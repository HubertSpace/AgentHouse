use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::io::Read;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ah_block::{Block, BlockAttachment, BlockKind, BlockState};
use ah_control::{
    AppSettingsSummary, BlockSummary, BrowserSessionSummary, ControlCommand, ControlErrorInfo,
    ControlEvent, ControlRequest, ControlResponse, ControlResult, ControlSnapshot,
    PersistedControlState, PersistedSessionRing, SessionSummary, SurfaceCapture, TerminalKeyInput,
    UiLanguagePreference, UiThemeSchemePreference, WindowSplitDirection, WindowSummary,
    WindowTabSummary, WorkspaceSummary,
};
use ah_core::{Actor, BlockId, SessionId, TabId, Timestamp, WindowId, WorkspaceId};
use ah_session::Session;
use ah_store::Store;
use ah_terminal::{
    PtySession, TerminalColor, TerminalEmulator, TerminalEvent, TerminalKey, TerminalKeyModifiers,
    TerminalScreenCell, TerminalScreenSnapshot, command_for_shell, input_sequence_for_key,
    paste_sequence_for_text,
};
#[cfg(test)]
use ah_web::HttpTextBrowserBackend;
use ah_web::{
    BrowserAction, BrowserActionResult, BrowserBackend, BrowserBackendSnapshot, BrowserFrameFormat,
    BrowserFrameSnapshot, BrowserInput, BrowserLoadStatus, BrowserSessionState,
    BrowserSurfaceSnapshot, PageSnapshot, ViewportSize,
};
use ah_web::{BrowserWorkerCommand, BrowserWorkerEvent};
#[cfg(target_os = "macos")]
use ah_webview_macos::WebViewProvider;
use ah_workspace::{LayoutMode, WindowContent, WindowTab, Workspace, WorkspaceWindow};
use gpui::{
    AlignItems, AnyElement, AnyView, App, AppContext, BorderStyle, Bounds, BoxShadow,
    ClipboardItem, Context, CursorStyle, Display, Div, Element, ElementId, FlexDirection,
    FocusHandle, FontStyle, FontWeight, GlobalElementId, Hsla, Image, ImageFormat, InputHandler,
    InspectorElementId, InteractiveElement, IntoElement, KeyDownEvent, LayoutId, Overflow,
    PaintQuad, ParentElement, PathPromptOptions, Pixels, Point, Render, Rgba, ScrollWheelEvent,
    ShapedLine, SharedString, Stateful, StatefulInteractiveElement, Style, Styled, TextAlign,
    TextRun, UTF16Selection, UnderlineStyle, WeakEntity, Window, actions, div, fill, font, img,
    point, prelude::FluentBuilder, px, relative, rgb, rgba, size,
};
use serde_json::json;
use tokio::sync::mpsc;

use crate::{control_server::QueuedControlRequest, refresh_app_menu};

actions!(
    agenthouse,
    [
        AboutAgentHouse,
        CheckForUpdates,
        OpenSettings,
        SetLanguageChinese,
        SetLanguageEnglish,
        SetThemeBlue,
        SetThemeCream,
        SetThemeGlass,
        SetThemeGreen,
        SetThemeLuxury,
        SetThemePurple,
        SetThemeRed,
        SetThemeSoft,
        SetThemeWarm,
        OpenWorkspaceFolder,
        RenameWorkspace,
        CloseWorkspace,
        NewTerminalTab,
        NewWebTab,
        SplitWindowRight,
        SplitWindowDown,
        QuitAgentHouse
    ]
);

const MAX_TERMINAL_OUTPUT_BYTES: usize = 12_000;
const MAX_BLOCK_OUTPUT_BYTES: usize = 64_000;
const MAX_BLOCK_DISPLAY_CHARS: usize = 8_000;
const MAX_DISPLAY_LINE_CHARS: usize = 140;
const MAX_FILE_PREVIEW_BYTES: usize = 64 * 1024;
const MAX_DIRECTORY_PREVIEW_ENTRIES: usize = 200;
const TERMINAL_GRID_COLS: usize = 120;
const TERMINAL_GRID_ROWS: usize = 36;
const TERMINAL_FONT_SIZE_PX: f32 = 12.0;
const TERMINAL_CELL_WIDTH_PX: f32 = 7.2;
const TERMINAL_CELL_HEIGHT_PX: f32 = 19.2;
const TERMINAL_GRID_INSET_PX: f32 = 0.0;
const WORKSPACE_RAIL_WIDTH_PX: f32 = 232.0;
const PANE_ACTION_SIZE_PX: f32 = 24.0;
const WINDOW_TAB_HEIGHT_PX: f32 = 36.0;
const WINDOW_TAB_ICON_SIZE_PX: f32 = 15.0;
const GLASS_RADIUS_SM_PX: f32 = 5.0;
const GLASS_RADIUS_MD_PX: f32 = 8.0;
const GLASS_WORKSPACE_HEADER_EMPTY_H_PX: f32 = 46.0;
const GLASS_WORKSPACE_SEARCH_H_PX: f32 = 31.0;
const GLASS_WORKSPACE_SEARCH_MARGIN_X_PX: f32 = 12.0;
const GLASS_WORKSPACE_SEARCH_MARGIN_B_PX: f32 = 10.0;
const GLASS_WORKSPACE_SEARCH_TEXT_SIZE_PX: f32 = 13.0;
const GLASS_WORKSPACE_LIST_PADDING_X_PX: f32 = 8.0;
const GLASS_WORKSPACE_CARD_RADIUS_PX: f32 = 8.0;
const GLASS_WORKSPACE_CARD_PADDING_PX: f32 = 10.0;
const GLASS_WORKSPACE_CARD_MARGIN_B_PX: f32 = 1.0;
const GLASS_WORKSPACE_NAME_GAP_PX: f32 = 7.0;
const GLASS_WORKSPACE_NAME_MARGIN_B_PX: f32 = 3.0;
const GLASS_WORKSPACE_NAME_TEXT_SIZE_PX: f32 = 14.0;
const GLASS_WORKSPACE_ICON_PX: f32 = 16.0;
const GLASS_WORKSPACE_ICON_RADIUS_PX: f32 = 3.0;
const GLASS_WORKSPACE_ICON_TEXT_SIZE_PX: f32 = 10.0;
const GLASS_WORKSPACE_META_INDENT_PX: f32 = 23.0;
const GLASS_WORKSPACE_META_MARGIN_B_PX: f32 = 1.0;
const GLASS_WORKSPACE_META_TEXT_SIZE_PX: f32 = 11.0;
const GLASS_WORKSPACE_FOOTER_PADDING_X_PX: f32 = 12.0;
const GLASS_WORKSPACE_FOOTER_PADDING_Y_PX: f32 = 10.0;
const GLASS_NEW_WORKSPACE_PADDING_X_PX: f32 = 10.0;
const GLASS_NEW_WORKSPACE_PADDING_Y_PX: f32 = 7.0;
const GLASS_NEW_WORKSPACE_GAP_PX: f32 = 6.0;
const GLASS_NEW_WORKSPACE_TEXT_SIZE_PX: f32 = 13.0;
const GLASS_NEW_WORKSPACE_PLUS_PX: f32 = 16.0;
const GLASS_NEW_WORKSPACE_PLUS_RADIUS_PX: f32 = 3.0;
const GLASS_NEW_WORKSPACE_PLUS_TEXT_SIZE_PX: f32 = 12.0;
const GLASS_PANE_SHADOW_WIDTH_PX: f32 = 1.0;
const GLASS_PANE_ACTIVE_SHADOW_WIDTH_PX: f32 = 1.0;
const GLASS_TABBAR_PADDING_X_PX: f32 = 6.0;
const GLASS_TABBAR_GAP_PX: f32 = 1.0;
const GLASS_TABBAR_TAB_PADDING_Y_PX: f32 = 3.0;
const GLASS_TAB_RADIUS_PX: f32 = 4.0;
const GLASS_TAB_PADDING_X_PX: f32 = 10.0;
const GLASS_TAB_PADDING_Y_PX: f32 = 3.0;
const GLASS_TAB_GAP_PX: f32 = 5.0;
const GLASS_TAB_TEXT_SIZE_PX: f32 = 12.5;
const GLASS_TAB_MAX_W_PX: f32 = 150.0;
const GLASS_TAB_CLOSE_PX: f32 = 14.0;
const GLASS_TAB_CLOSE_TEXT_SIZE_PX: f32 = 11.0;
const GLASS_PANE_ACTION_COUNT: usize = 4;
const GLASS_PANE_ACTION_GROUP_GAP_PX: f32 = 1.0;
const GLASS_PANE_ACTION_GROUP_MARGIN_L_PX: f32 = 6.0;
const GLASS_PANE_ACTION_GROUP_PADDING_L_PX: f32 = 6.0;
const GLASS_PANE_ACTION_RADIUS_PX: f32 = 3.0;
const GLASS_BROWSER_ADDRESS_GAP_PX: f32 = 6.0;
const GLASS_BROWSER_ADDRESS_PADDING_X_PX: f32 = 8.0;
const GLASS_BROWSER_ADDRESS_PADDING_Y_PX: f32 = 6.0;
const GLASS_BROWSER_ADDRESS_H_PX: f32 = 24.0;
const GLASS_BROWSER_ADDRESS_INPUT_PADDING_X_PX: f32 = 10.0;
const GLASS_BROWSER_NAV_GROUP_GAP_PX: f32 = 1.0;
const GLASS_BROWSER_NAV_SIZE_PX: f32 = 22.0;
const GLASS_BROWSER_NAV_RADIUS_PX: f32 = 3.0;
const GLASS_BROWSER_NAV_TEXT_SIZE_PX: f32 = 11.0;
const GLASS_BROWSER_STATUS_DOT_PX: f32 = 4.0;
const GLASS_BROWSER_STATUS_GAP_PX: f32 = 4.0;
const GLASS_BROWSER_STATUS_PADDING_X_PX: f32 = 8.0;
const GLASS_BROWSER_STATUS_PADDING_Y_PX: f32 = 3.0;
const GLASS_BROWSER_STATUS_TEXT_SIZE_PX: f32 = 9.0;
const GLASS_BROWSER_STATUS_MAX_W_PX: f32 = 96.0;
const GLASS_TERMINAL_HEADER_GAP_PX: f32 = 6.0;
const GLASS_TERMINAL_HEADER_PADDING_X_PX: f32 = 10.0;
const GLASS_TERMINAL_HEADER_PADDING_Y_PX: f32 = 5.0;
const GLASS_TERMINAL_HEADER_TITLE_SIZE_PX: f32 = 11.0;
const GLASS_TERMINAL_BODY_PADDING_X_PX: f32 = 12.0;
const GLASS_TERMINAL_BODY_PADDING_Y_PX: f32 = 10.0;
const GLASS_HEADER_BADGE_PADDING_X_PX: f32 = 7.0;
const GLASS_HEADER_BADGE_PADDING_Y_PX: f32 = 1.0;
const GLASS_HEADER_BADGE_TEXT_SIZE_PX: f32 = 10.0;
const GLASS_BROWSER_PAGE_MAX_W_PX: f32 = 640.0;
const GLASS_BROWSER_PAGE_PADDING_X_PX: f32 = 36.0;
const GLASS_BROWSER_PAGE_PADDING_Y_PX: f32 = 28.0;
const GLASS_BROWSER_PAGE_TITLE_SIZE_PX: f32 = 22.0;
const GLASS_BROWSER_PAGE_TITLE_MARGIN_B_PX: f32 = 5.0;
const GLASS_BROWSER_PAGE_SUBTITLE_SIZE_PX: f32 = 14.0;
const GLASS_BROWSER_PAGE_SUBTITLE_LINE_HEIGHT_PX: f32 = 21.0;
const GLASS_BROWSER_PAGE_SUBTITLE_MARGIN_B_PX: f32 = 20.0;
const GLASS_BROWSER_CARD_GAP_PX: f32 = 8.0;
const GLASS_BROWSER_CARD_RADIUS_PX: f32 = 6.0;
const GLASS_BROWSER_CARD_PADDING_PX: f32 = 12.0;
const GLASS_BROWSER_CARD_ICON_PX: f32 = 14.0;
const GLASS_BROWSER_CARD_TITLE_GAP_PX: f32 = 6.0;
const GLASS_BROWSER_CARD_TITLE_MARGIN_B_PX: f32 = 3.0;
const GLASS_BROWSER_CARD_TITLE_SIZE_PX: f32 = 13.0;
const GLASS_BROWSER_CARD_BODY_SIZE_PX: f32 = 12.0;
const GLASS_BROWSER_CARD_BODY_LINE_HEIGHT_PX: f32 = 16.8;
const GLASS_TOOLTIP_PADDING_X_PX: f32 = 8.0;
const GLASS_TOOLTIP_PADDING_Y_PX: f32 = 5.0;
const GLASS_TOOLTIP_TEXT_SIZE_PX: f32 = 11.0;
const UI_FONT_SANS: &str = "Geist";
const UI_FONT_MONO: &str = "Geist Mono";
const MAX_WORKSPACE_PANES: usize = 16;
const INLINE_WORKSPACE_CREATE_LIMIT: usize = 6;
const DEFAULT_BROWSER_URL: &str = "https://www.baidu.com/";
const DEFAULT_UI_THEME_SCHEME: UiThemeSchemePreference = UiThemeSchemePreference::Glass;
const FIXED_UI_THEME_MODE: &str = "light";

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
struct AgentHouseTheme {
    app_bg: Rgba,
    rail_bg: Rgba,
    board_bg: Rgba,
    panel_bg: Rgba,
    panel_alt_bg: Rgba,
    tabbar_bg: Rgba,
    panel_inset_bg: Rgba,
    hover_bg: Rgba,
    border: Rgba,
    pane_frame_border: Rgba,
    border_strong: Rgba,
    border_term: Rgba,
    active_bg: Rgba,
    focus_bg: Rgba,
    active_border: Rgba,
    text: Rgba,
    text_muted: Rgba,
    text_subtle: Rgba,
    accent: Rgba,
    command_prompt: Rgba,
    warning: Rgba,
    success: Rgba,
    error: Rgba,
    error_bg: Rgba,
    error_border: Rgba,
    terminal_bg: Rgba,
    terminal_fg: Rgba,
    terminal_panel_bg: Rgba,
    terminal_input_bg: Rgba,
    terminal_placeholder: Rgba,
    ring_border: Rgba,
    inactive_pane_overlay: Rgba,
    tag_blue_bg: Rgba,
    tag_blue_text: Rgba,
    tag_green_bg: Rgba,
    tag_green_text: Rgba,
    tag_amber_bg: Rgba,
    tag_amber_text: Rgba,
    tag_red_bg: Rgba,
    tag_red_text: Rgba,
}

impl AgentHouseTheme {
    fn for_scheme(scheme: UiThemeSchemePreference) -> Self {
        match scheme {
            UiThemeSchemePreference::Cream => Self::from_light_tokens(LightThemeTokens {
                app_bg: 0xf4f1eb,
                rail_bg: 0xf1eee8,
                panel_bg: 0xfaf8f5,
                panel_alt_bg: 0xeeeae4,
                tabbar_bg: 0xe8e4de,
                panel_inset_bg: 0xe0dcd6,
                border: 0xddd9d2,
                border_strong: 0xc0a898,
                active_bg: 0xdcd8d2,
                focus_bg: 0xd2cec8,
                active_border: 0xc04030,
                text: 0x1c1a18,
                text_muted: 0x5c5850,
                text_subtle: 0x948e86,
                terminal_bg: 0xebe7e0,
                terminal_panel_bg: 0xe2ded8,
                terminal_input_bg: 0xd8d4ce,
                accent: 0xc04030,
                success: 0x3d8050,
                warning: 0x8a7428,
                error: 0xb04040,
            }),
            UiThemeSchemePreference::Warm => Self::from_light_tokens(LightThemeTokens {
                app_bg: 0xf6f4ee,
                rail_bg: 0xf3f0ea,
                panel_bg: 0xfbf9f6,
                panel_alt_bg: 0xf0ede6,
                tabbar_bg: 0xeae6de,
                panel_inset_bg: 0xe2ded4,
                border: 0xdcd8ce,
                border_strong: 0xb8a890,
                active_bg: 0xdcd8ce,
                focus_bg: 0xd2cec2,
                active_border: 0xb89030,
                text: 0x1a1814,
                text_muted: 0x5c5648,
                text_subtle: 0x948c7e,
                terminal_bg: 0xe9e5dc,
                terminal_panel_bg: 0xe0dcd2,
                terminal_input_bg: 0xd6d2c8,
                accent: 0xb89030,
                success: 0x3d8050,
                warning: 0x8a7428,
                error: 0xb04040,
            }),
            UiThemeSchemePreference::Blue => Self::from_light_tokens(LightThemeTokens {
                app_bg: 0xf2f4f8,
                rail_bg: 0xeff1f6,
                panel_bg: 0xfafbfd,
                panel_alt_bg: 0xedeff4,
                tabbar_bg: 0xe6e8ee,
                panel_inset_bg: 0xdee0e8,
                border: 0xd8dae2,
                border_strong: 0x98a0b8,
                active_bg: 0xd6d8e0,
                focus_bg: 0xccced8,
                active_border: 0x3878b0,
                text: 0x181a1e,
                text_muted: 0x505868,
                text_subtle: 0x8e96a6,
                terminal_bg: 0xe8eaef,
                terminal_panel_bg: 0xe0e2e8,
                terminal_input_bg: 0xd6d8e0,
                accent: 0x3878b0,
                success: 0x3d8050,
                warning: 0x8a7428,
                error: 0xb04040,
            }),
            UiThemeSchemePreference::Green => Self::from_light_tokens(LightThemeTokens {
                app_bg: 0xf2f4f2,
                rail_bg: 0xeff2ef,
                panel_bg: 0xfafbfa,
                panel_alt_bg: 0xecf0ec,
                tabbar_bg: 0xe4e8e4,
                panel_inset_bg: 0xdcdddc,
                border: 0xd6d8d6,
                border_strong: 0x8ca890,
                active_bg: 0xd4d6d4,
                focus_bg: 0xc8ccc8,
                active_border: 0x3a7848,
                text: 0x181c18,
                text_muted: 0x505c50,
                text_subtle: 0x8e968e,
                terminal_bg: 0xe6e8e6,
                terminal_panel_bg: 0xdee0de,
                terminal_input_bg: 0xd4d6d4,
                accent: 0x3a7848,
                success: 0x3a7848,
                warning: 0x8a7428,
                error: 0xb04040,
            }),
            UiThemeSchemePreference::Red => Self::from_light_tokens(LightThemeTokens {
                app_bg: 0xf4f2f2,
                rail_bg: 0xf1efef,
                panel_bg: 0xfbfafa,
                panel_alt_bg: 0xf0eded,
                tabbar_bg: 0xe8e4e4,
                panel_inset_bg: 0xe0dcdc,
                border: 0xd8d4d4,
                border_strong: 0xb89090,
                active_bg: 0xd8d4d4,
                focus_bg: 0xcecaca,
                active_border: 0xb04040,
                text: 0x1c1818,
                text_muted: 0x5c5050,
                text_subtle: 0x968e8e,
                terminal_bg: 0xe8e4e4,
                terminal_panel_bg: 0xe0dcdc,
                terminal_input_bg: 0xd6d2d2,
                accent: 0xb04040,
                success: 0x3d8050,
                warning: 0x8a7428,
                error: 0xb04040,
            }),
            UiThemeSchemePreference::Purple => Self::from_light_tokens(LightThemeTokens {
                app_bg: 0xf2f2f6,
                rail_bg: 0xefeff4,
                panel_bg: 0xfafafd,
                panel_alt_bg: 0xededf3,
                tabbar_bg: 0xe6e6ee,
                panel_inset_bg: 0xdedee8,
                border: 0xd8d8e2,
                border_strong: 0x9a92b8,
                active_bg: 0xd6d6e0,
                focus_bg: 0xccccd8,
                active_border: 0x7a5fb0,
                text: 0x1a1820,
                text_muted: 0x565068,
                text_subtle: 0x928ea6,
                terminal_bg: 0xe8e8ef,
                terminal_panel_bg: 0xe0e0e8,
                terminal_input_bg: 0xd6d6e0,
                accent: 0x7a5fb0,
                success: 0x3d8050,
                warning: 0x8a7428,
                error: 0xb04040,
            }),
            UiThemeSchemePreference::Glass => Self::from_light_tokens(LightThemeTokens {
                app_bg: 0xfafafa,
                rail_bg: 0xf6f6f6,
                panel_bg: 0xffffff,
                panel_alt_bg: 0xf4f4f4,
                tabbar_bg: 0xf0f0f0,
                panel_inset_bg: 0xe8e8e8,
                border: 0xe2e2e2,
                border_strong: 0xa0a0a0,
                active_bg: 0xe4e4e4,
                focus_bg: 0xdcdcdc,
                active_border: 0x1a1a1a,
                text: 0x18181a,
                text_muted: 0x5a5a62,
                text_subtle: 0x9a9aa2,
                terminal_bg: 0xf2f2f2,
                terminal_panel_bg: 0xeaeaea,
                terminal_input_bg: 0xe4e4e4,
                accent: 0x1a1a1a,
                success: 0x2d7d46,
                warning: 0x8a7220,
                error: 0xb03030,
            }),
            UiThemeSchemePreference::Luxury => Self::from_light_tokens(LightThemeTokens {
                app_bg: 0xf6f3ed,
                rail_bg: 0xf2eee6,
                panel_bg: 0xfcfaf6,
                panel_alt_bg: 0xeee8dc,
                tabbar_bg: 0xe6decf,
                panel_inset_bg: 0xdcd3c2,
                border: 0xd8cfbd,
                border_strong: 0xb8a060,
                active_bg: 0xddd3bf,
                focus_bg: 0xd0c2a8,
                active_border: 0xb8a060,
                text: 0x1f1a12,
                text_muted: 0x5f5644,
                text_subtle: 0x9a907e,
                terminal_bg: 0xeae2d4,
                terminal_panel_bg: 0xe2d8c8,
                terminal_input_bg: 0xd8ccb8,
                accent: 0xb8a060,
                success: 0x3d8050,
                warning: 0x9a7820,
                error: 0xb04040,
            }),
            UiThemeSchemePreference::Soft => Self::from_light_tokens(LightThemeTokens {
                app_bg: 0xf4f4f6,
                rail_bg: 0xf0f0f3,
                panel_bg: 0xfbfbfd,
                panel_alt_bg: 0xeeeef2,
                tabbar_bg: 0xe8e8ee,
                panel_inset_bg: 0xe0e0e6,
                border: 0xdadade,
                border_strong: 0x9aa0b2,
                active_bg: 0xd8d8de,
                focus_bg: 0xceced6,
                active_border: 0x7080a0,
                text: 0x1a1a1f,
                text_muted: 0x565a66,
                text_subtle: 0x9296a2,
                terminal_bg: 0xeaeaec,
                terminal_panel_bg: 0xe2e2e6,
                terminal_input_bg: 0xd8d8de,
                accent: 0x7080a0,
                success: 0x3d8050,
                warning: 0x8a7428,
                error: 0xb04040,
            }),
        }
    }

    #[cfg(test)]
    fn glass_magazine() -> Self {
        Self::for_scheme(UiThemeSchemePreference::Glass)
    }

    fn from_light_tokens(tokens: LightThemeTokens) -> Self {
        let tag_blue_bg = rgb(0xe1f0fa);
        let tag_blue_text = rgb(0x1f6c9f);
        let tag_green_bg = rgb(0xedf3ec);
        let tag_green_text = rgb(0x346538);
        let tag_amber_bg = rgb(0xfbf3db);
        let tag_amber_text = rgb(0x956400);
        let tag_red_bg = rgb(0xfdebec);
        let tag_red_text = rgb(0x9f2f2d);

        Self {
            app_bg: rgb(tokens.app_bg),
            rail_bg: rgb(tokens.rail_bg),
            board_bg: rgb(tokens.app_bg),
            panel_bg: rgb(tokens.panel_bg),
            panel_alt_bg: rgb(tokens.panel_alt_bg),
            tabbar_bg: rgb(tokens.tabbar_bg),
            panel_inset_bg: rgb(tokens.panel_inset_bg),
            hover_bg: rgba(0x00000008),
            border: rgb(tokens.border),
            pane_frame_border: rgb(tokens.border),
            border_strong: rgb(tokens.border_strong),
            border_term: rgba(0x0000000f),
            active_bg: rgb(tokens.active_bg),
            focus_bg: rgb(tokens.focus_bg),
            active_border: rgb(tokens.active_border),
            text: rgb(tokens.text),
            text_muted: rgb(tokens.text_muted),
            text_subtle: rgb(tokens.text_subtle),
            accent: rgb(tokens.accent),
            command_prompt: rgb(tokens.success),
            warning: rgb(tokens.warning),
            success: rgb(tokens.success),
            error: rgb(tokens.error),
            error_bg: tag_red_bg,
            error_border: rgb(tokens.error),
            terminal_bg: rgb(tokens.terminal_bg),
            terminal_fg: rgb(tokens.text),
            terminal_panel_bg: rgb(tokens.terminal_panel_bg),
            terminal_input_bg: rgb(tokens.terminal_input_bg),
            terminal_placeholder: rgba(0x00000061),
            ring_border: rgb(0xffffff),
            inactive_pane_overlay: rgba(0xffffff66),
            tag_blue_bg,
            tag_blue_text,
            tag_green_bg,
            tag_green_text,
            tag_amber_bg,
            tag_amber_text,
            tag_red_bg,
            tag_red_text,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct LightThemeTokens {
    app_bg: u32,
    rail_bg: u32,
    panel_bg: u32,
    panel_alt_bg: u32,
    tabbar_bg: u32,
    panel_inset_bg: u32,
    border: u32,
    border_strong: u32,
    active_bg: u32,
    focus_bg: u32,
    active_border: u32,
    text: u32,
    text_muted: u32,
    text_subtle: u32,
    terminal_bg: u32,
    terminal_panel_bg: u32,
    terminal_input_bg: u32,
    accent: u32,
    success: u32,
    warning: u32,
    error: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UiLanguage {
    ZhCn,
    En,
}

impl Default for UiLanguage {
    fn default() -> Self {
        Self::ZhCn
    }
}

impl UiLanguage {
    fn from_environment() -> Self {
        match std::env::var("AGENTHOUSE_UI_LANGUAGE")
            .ok()
            .as_deref()
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("en") | Some("en-us") | Some("english") => Self::En,
            Some("zh") | Some("zh-cn") | Some("cn") | Some("chinese") => Self::ZhCn,
            _ => Self::default(),
        }
    }

    fn select(self, zh_cn: &'static str, en: &'static str) -> &'static str {
        match self {
            Self::ZhCn => zh_cn,
            Self::En => en,
        }
    }

    fn preference(self) -> UiLanguagePreference {
        match self {
            Self::ZhCn => UiLanguagePreference::ZhCn,
            Self::En => UiLanguagePreference::En,
        }
    }

    fn from_preference(preference: UiLanguagePreference) -> Self {
        match preference {
            UiLanguagePreference::ZhCn => Self::ZhCn,
            UiLanguagePreference::En => Self::En,
        }
    }
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
struct BlockRow {
    title: SharedString,
    block: Block,
}

#[derive(Clone, Debug)]
struct NotificationRing {
    state: RingState,
    summary: SharedString,
    unread_count: u32,
}

#[derive(Clone, Debug)]
enum RingState {
    Idle,
    Running,
    Complete,
    Error,
}

impl RingState {
    fn from_label(label: &str) -> Self {
        match label {
            "running" => Self::Running,
            "complete" => Self::Complete,
            "error" => Self::Error,
            "idle" => Self::Idle,
            _ => Self::Idle,
        }
    }
}

impl NotificationRing {
    fn idle(summary: impl Into<SharedString>) -> Self {
        Self {
            state: RingState::Idle,
            summary: summary.into(),
            unread_count: 0,
        }
    }

    fn from_persisted(ring: PersistedSessionRing) -> Self {
        Self {
            state: RingState::from_label(&ring.state),
            summary: ring.summary.into(),
            unread_count: ring.unread_count,
        }
    }

    fn update(&mut self, state: RingState, summary: impl Into<SharedString>) {
        self.state = state;
        self.summary = summary.into();
        self.unread_count = match self.state {
            RingState::Idle => 0,
            RingState::Running | RingState::Complete | RingState::Error => {
                self.unread_count.saturating_add(1)
            }
        };
    }

    fn acknowledge(&mut self) {
        self.unread_count = 0;
    }
}

#[derive(Default)]
struct BrowserAddressEditState {
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
}

impl BrowserAddressEditState {
    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn select_all(&mut self, text: &str) {
        self.selected_range = 0..text.len();
        self.selection_reversed = false;
        self.marked_range = None;
    }

    fn move_to(&mut self, offset: usize) {
        self.selected_range = offset..offset;
        self.selection_reversed = false;
    }

    fn clear_layout(&mut self) {
        self.last_layout = None;
        self.last_bounds = None;
    }

    fn clamp_to_text(&mut self, text: &str) {
        self.selected_range.start = clamp_to_char_boundary(text, self.selected_range.start);
        self.selected_range.end = clamp_to_char_boundary(text, self.selected_range.end);
        if self.selected_range.start > self.selected_range.end {
            self.selected_range = self.selected_range.end..self.selected_range.start;
            self.selection_reversed = !self.selection_reversed;
        }
        self.marked_range = self.marked_range.take().and_then(|range| {
            let start = clamp_to_char_boundary(text, range.start);
            let end = clamp_to_char_boundary(text, range.end);
            (start <= end).then_some(start..end)
        });
    }
}

#[derive(Clone, Default)]
struct BrowserAddressRenderState {
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    cursor_offset: usize,
}

#[derive(Clone, Debug, serde::Serialize)]
struct FilePreviewSnapshot {
    path: PathBuf,
    kind: String,
    status: String,
    text: Option<String>,
    byte_count: Option<u64>,
    truncated: bool,
}

#[derive(Clone, Debug, serde::Serialize)]
struct WebPreviewSnapshot {
    url: String,
    status: String,
    title: Option<String>,
    text: Option<String>,
    http_status: Option<u16>,
    byte_count: Option<usize>,
    truncated: bool,
    error: Option<String>,
    captured_at: Timestamp,
}

impl WebPreviewSnapshot {
    fn pending(url: &str) -> Self {
        Self {
            url: url.to_string(),
            status: "web preview not fetched".to_string(),
            title: None,
            text: None,
            http_status: None,
            byte_count: None,
            truncated: false,
            error: None,
            captured_at: Timestamp::now(),
        }
    }
}

impl From<PageSnapshot> for WebPreviewSnapshot {
    fn from(snapshot: PageSnapshot) -> Self {
        let status = match snapshot.status {
            Some(status) if snapshot.truncated => format!("http {status}, preview truncated"),
            Some(status) => format!("http {status}"),
            None => "web preview ready".to_string(),
        };
        Self {
            url: snapshot.url,
            status,
            title: snapshot.title,
            text: snapshot.text,
            http_status: snapshot.status,
            byte_count: snapshot.byte_count,
            truncated: snapshot.truncated,
            error: None,
            captured_at: snapshot.captured_at,
        }
    }
}

// ─── Native WebView Backend ─────────────────────────────────────────

/// Browser backend using the system-native WebView (WKWebView on macOS).
/// Wraps `ah_webview_macos::WKWebViewProvider` to implement `BrowserBackend`.
#[cfg(target_os = "macos")]
#[derive(Debug)]
struct NativeWebViewBackend {
    provider: ah_webview_macos::WKWebViewProvider,
}

#[cfg(target_os = "macos")]
impl NativeWebViewBackend {
    fn new() -> Result<Self, String> {
        let provider = ah_webview_macos::WKWebViewProvider::new()?;
        Ok(Self { provider })
    }
}

#[cfg(target_os = "macos")]
impl BrowserBackend for NativeWebViewBackend {
    fn engine(&self) -> ah_web::BrowserEngine {
        ah_web::BrowserEngine::Native
    }

    fn open(&mut self, url: &str) -> Result<(), ah_web::WebError> {
        self.provider.navigate(url)
    }

    fn navigate(&mut self, url: &str) -> Result<(), ah_web::WebError> {
        self.provider.navigate(url)
    }

    fn reload(&mut self) -> Result<(), ah_web::WebError> {
        self.provider.reload()
    }

    fn go_back(&mut self) -> Result<(), ah_web::WebError> {
        self.provider.go_back()
    }

    fn go_forward(&mut self) -> Result<(), ah_web::WebError> {
        self.provider.go_forward()
    }

    fn resize(&mut self, size: ViewportSize) -> Result<(), ah_web::WebError> {
        self.provider.resize(size.width, size.height)
    }

    fn input(&mut self, _input: BrowserInput) -> Result<(), ah_web::WebError> {
        // Native webview handles input directly when embedded as NSView.
        // Programmatic input would go through evaluate_js.
        Ok(())
    }

    fn action(&mut self, action: &BrowserAction) -> Result<Option<String>, ah_web::WebError> {
        match action {
            BrowserAction::Navigate { url } => {
                self.provider.navigate(url)?;
                Ok(None)
            }
            BrowserAction::Reload => {
                self.provider.reload()?;
                Ok(None)
            }
            BrowserAction::Back => {
                self.provider.go_back()?;
                Ok(None)
            }
            BrowserAction::Forward => {
                self.provider.go_forward()?;
                Ok(None)
            }
            BrowserAction::Click { selector } => {
                let js = format!(
                    "(function(){{ var el = document.querySelector('{}'); if(el){{ el.click(); return 'clicked'; }} return 'not found'; }})()",
                    selector.replace('\'', "\\'")
                );
                self.provider.evaluate_js(&js)
            }
            BrowserAction::Fill { selector, value } => {
                let js = format!(
                    "(function(){{ var el = document.querySelector('{}'); if(el){{ el.value = '{}'; el.dispatchEvent(new Event('change')); return 'filled'; }} return 'not found'; }})()",
                    selector.replace('\'', "\\'"),
                    value.replace('\'', "\\'")
                );
                self.provider.evaluate_js(&js)
            }
            BrowserAction::Type { selector, text } => {
                let js = format!(
                    "(function(){{ var el = document.querySelector('{}'); if(el){{ el.focus(); el.value += '{}'; el.dispatchEvent(new Event('input')); return 'typed'; }} return 'not found'; }})()",
                    selector.replace('\'', "\\'"),
                    text.replace('\'', "\\'")
                );
                self.provider.evaluate_js(&js)
            }
            BrowserAction::PressKey { key, selector: _ } => {
                let js = format!(
                    "(function(){{ var ev = new KeyboardEvent('keydown', {{key: '{}', bubbles: true}}); document.activeElement.dispatchEvent(ev); return 'pressed'; }})()",
                    key.replace('\'', "\\'")
                );
                self.provider.evaluate_js(&js)
            }
            BrowserAction::SelectOption { selector, value } => {
                let js = format!(
                    "(function(){{ var el = document.querySelector('{}'); if(el){{ el.value = '{}'; el.dispatchEvent(new Event('change')); return 'selected'; }} return 'not found'; }})()",
                    selector.replace('\'', "\\'"),
                    value.replace('\'', "\\'")
                );
                self.provider.evaluate_js(&js)
            }
            BrowserAction::Evaluate { expression } => self.provider.evaluate_js(expression),
            BrowserAction::Snapshot => Ok(None),
        }
    }

    fn snapshot(&mut self) -> Result<BrowserBackendSnapshot, ah_web::WebError> {
        let url = self.provider.current_url();
        let title = self.provider.title();

        // Drain navigation events to update loaded state
        let events = self.provider.drain_navigation_events();
        for event in events {
            tracing::debug!(?event, "native webview navigation event");
        }

        let page = PageSnapshot {
            url,
            title,
            text: None,
            status: None,
            byte_count: None,
            truncated: false,
            captured_at: ah_core::Timestamp::now(),
        };

        // WKWebView can keep reporting isLoading for long-running pages.
        // Once a real URL exists, attempt snapshots and let the provider
        // return a typed error until WebKit has a drawable frame.
        let frame = if !page.url.is_empty() {
            match self.provider.screenshot() {
                Ok(snap) => Some(BrowserFrameSnapshot::new(
                    BrowserFrameFormat::Png,
                    snap.width,
                    snap.height,
                    snap.png_data,
                )),
                Err(error) => {
                    tracing::debug!(
                        ?error,
                        loaded = self.provider.is_loaded(),
                        "native webview snapshot unavailable"
                    );
                    None
                }
            }
        } else {
            None
        };

        Ok(BrowserBackendSnapshot {
            page: Some(page),
            frame,
        })
    }
}

struct BrowserRuntime {
    state: BrowserSessionState,
    frame: Option<BrowserFrameSnapshot>,
    commands: std::sync::mpsc::Sender<BrowserWorkerCommand>,
    events: mpsc::UnboundedReceiver<BrowserWorkerEvent>,
    pending_status: Option<String>,
}

impl BrowserRuntime {
    #[cfg(test)]
    fn new_text_preview(title: impl Into<String>, url: impl Into<String>) -> Self {
        Self::new(title, url, Box::new(HttpTextBrowserBackend::new()), None)
    }

    /// Create a browser runtime using the system-native WebView.
    /// AgentHouse 0.1.0 is macOS-only; native WebView failure is surfaced
    /// instead of falling back to a different browser backend.
    fn new_native(
        title: impl Into<String>,
        url: impl Into<String>,
        wake_tx: mpsc::UnboundedSender<()>,
    ) -> Result<Self, String> {
        let backend = NativeWebViewBackend::new().map_err(|error| {
            format!("native WKWebView backend is required for AgentHouse 0.1.0: {error}")
        })?;
        tracing::info!("native webview loaded; using system WKWebView backend");
        Ok(Self::new(title, url, Box::new(backend), Some(wake_tx)))
    }

    fn new(
        title: impl Into<String>,
        url: impl Into<String>,
        backend: Box<dyn BrowserBackend + Send>,
        wake_tx: Option<mpsc::UnboundedSender<()>>,
    ) -> Self {
        let url = url.into();
        let mut state = BrowserSessionState::new(title, url.clone(), backend.engine());
        state.mark_loading();
        let (commands, command_rx) = std::sync::mpsc::channel();
        let (events_tx, events) = mpsc::unbounded_channel();
        BrowserWorker::spawn(backend, command_rx, events_tx, wake_tx);
        let _ = commands.send(BrowserWorkerCommand::Open { url });
        Self {
            state,
            frame: None,
            commands,
            events,
            pending_status: Some("browser loading".to_string()),
        }
    }

    fn session_id(&self) -> SessionId {
        self.state.id
    }

    fn preview_snapshot(&self) -> WebPreviewSnapshot {
        if let Some(snapshot) = &self.state.last_snapshot {
            return WebPreviewSnapshot::from(snapshot.clone());
        }

        if let Some(error) = &self.state.last_error {
            return WebPreviewSnapshot {
                url: self.state.current_url.clone(),
                status: format!("browser failed: {error}"),
                title: Some(self.state.title.clone()),
                text: None,
                http_status: None,
                byte_count: None,
                truncated: false,
                error: Some(error.clone()),
                captured_at: self.state.updated_at,
            };
        }

        WebPreviewSnapshot::pending(&self.state.current_url)
    }

    fn surface_snapshot(&self) -> BrowserSurfaceSnapshot {
        BrowserSurfaceSnapshot {
            session: self.state.clone(),
            page: self.state.last_snapshot.clone(),
            frame: self.frame.clone(),
        }
    }

    fn apply_backend_snapshot(&mut self, snapshot: BrowserBackendSnapshot) -> String {
        let mut status = match self.state.status {
            BrowserLoadStatus::Loading => "browser loading".to_string(),
            BrowserLoadStatus::Idle => "browser idle".to_string(),
            BrowserLoadStatus::Loaded => "browser ready".to_string(),
            BrowserLoadStatus::Failed => "browser failed".to_string(),
        };
        if let Some(page) = snapshot.page {
            status = WebPreviewSnapshot::from(page.clone()).status;
            self.state.apply_snapshot(page);
        } else {
            self.state.status = BrowserLoadStatus::Loaded;
            self.state.last_error = None;
            self.state.updated_at = Timestamp::now();
        }
        self.frame = snapshot.frame;
        status
    }

    fn apply_error(&mut self, error: String) {
        self.state.apply_error(error);
        self.pending_status = None;
    }

    fn queue(&self, command: BrowserWorkerCommand) -> Result<(), String> {
        self.commands
            .send(command)
            .map_err(|_| "browser worker is not running".to_string())
    }

    fn drain_events(&mut self) -> bool {
        let mut did_update = false;
        while let Ok(event) = self.events.try_recv() {
            did_update = true;
            match event {
                BrowserWorkerEvent::Snapshot(snapshot) => {
                    let status = self.apply_backend_snapshot(snapshot);
                    self.pending_status = Some(status);
                }
                BrowserWorkerEvent::Error(error) => {
                    self.apply_error(error);
                }
                BrowserWorkerEvent::ActionResult(value) => {
                    self.pending_status = Some(match value {
                        Some(value) if !value.is_empty() => value,
                        _ => "browser action applied".to_string(),
                    });
                }
                BrowserWorkerEvent::Shutdown => {
                    self.pending_status = Some("browser worker stopped".to_string());
                }
                BrowserWorkerEvent::Frame(frame) => {
                    self.frame = Some(frame);
                    if self.state.status == BrowserLoadStatus::Loading {
                        self.state.status = BrowserLoadStatus::Loaded;
                    }
                }
            }
        }
        did_update
    }

    fn navigate(&mut self, url: impl Into<String>) -> Result<String, String> {
        let url = url.into();
        self.queue(BrowserWorkerCommand::Navigate { url: url.clone() })?;
        self.state.navigate_to(url);
        self.pending_status = Some("browser loading".to_string());
        Ok("browser loading".to_string())
    }

    fn apply_action(&mut self, action: &BrowserAction) -> Result<Option<String>, String> {
        match action {
            BrowserAction::Snapshot => {}
            BrowserAction::Navigate { url } => self.state.navigate_to(url.clone()),
            BrowserAction::Reload | BrowserAction::Back | BrowserAction::Forward => {
                self.state.mark_loading();
            }
            _ => {}
        }
        self.queue(BrowserWorkerCommand::Action(action.clone()))?;
        self.pending_status = Some("browser action queued".to_string());
        Ok(None)
    }

    fn input(&mut self, input: BrowserInput) -> Result<String, String> {
        self.queue(BrowserWorkerCommand::Input(input))?;
        self.pending_status = Some("browser input queued".to_string());
        Ok("browser input queued".to_string())
    }

    fn resize(&mut self, viewport: ViewportSize) -> Result<(), String> {
        self.queue(BrowserWorkerCommand::Resize(viewport))?;
        self.state.resize(viewport);
        Ok(())
    }
}

struct BrowserWorker;

impl BrowserWorker {
    /// Duration of continuous frame streaming after a navigation or input command.
    const STREAMING_TIMEOUT: Duration = Duration::from_secs(5);
    /// Interval between frame polls during streaming mode (~20 FPS).
    const POLL_INTERVAL: Duration = Duration::from_millis(50);

    fn spawn(
        mut backend: Box<dyn BrowserBackend + Send>,
        commands: std::sync::mpsc::Receiver<BrowserWorkerCommand>,
        events: mpsc::UnboundedSender<BrowserWorkerEvent>,
        wake_tx: Option<mpsc::UnboundedSender<()>>,
    ) {
        thread::spawn(move || {
            let mut streaming_until = None::<Instant>;

            loop {
                // Expire streaming when the deadline passes.
                if streaming_until.is_some_and(|until| Instant::now() >= until) {
                    streaming_until = None;
                }

                // Idle mode: block until a command arrives (zero CPU usage).
                // Streaming mode: poll with timeout so we can push frame updates.
                let command = match streaming_until {
                    Some(_) => match commands.recv_timeout(Self::POLL_INTERVAL) {
                        Ok(cmd) => cmd,
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                            Self::poll_frame(backend.as_mut(), &events, wake_tx.as_ref());
                            continue;
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    },
                    None => match commands.recv() {
                        Ok(cmd) => cmd,
                        Err(_) => break,
                    },
                };

                let should_stream = !matches!(
                    &command,
                    BrowserWorkerCommand::Shutdown | BrowserWorkerCommand::Snapshot
                );

                if !Self::handle_command(command, backend.as_mut(), &events, wake_tx.as_ref()) {
                    break;
                }

                if should_stream {
                    streaming_until = Some(Instant::now() + Self::STREAMING_TIMEOUT);
                }
            }

            Self::send_event(BrowserWorkerEvent::Shutdown, &events, wake_tx.as_ref());
        });
    }

    /// Poll the backend for a new frame and push it as a Frame event.
    fn poll_frame(
        backend: &mut (dyn BrowserBackend + Send),
        events: &mpsc::UnboundedSender<BrowserWorkerEvent>,
        wake_tx: Option<&mpsc::UnboundedSender<()>>,
    ) {
        if let Ok(snapshot) = backend.snapshot() {
            if let Some(frame) = snapshot.frame {
                Self::send_event(BrowserWorkerEvent::Frame(frame), events, wake_tx);
            }
        }
    }

    fn handle_command(
        command: BrowserWorkerCommand,
        backend: &mut (dyn BrowserBackend + Send),
        events: &mpsc::UnboundedSender<BrowserWorkerEvent>,
        wake_tx: Option<&mpsc::UnboundedSender<()>>,
    ) -> bool {
        match command {
            BrowserWorkerCommand::Open { url } => {
                Self::send_result(
                    backend.open(&url).and_then(|()| backend.snapshot()),
                    events,
                    wake_tx,
                );
            }
            BrowserWorkerCommand::Navigate { url } => {
                Self::send_result(
                    backend.navigate(&url).and_then(|()| backend.snapshot()),
                    events,
                    wake_tx,
                );
            }
            BrowserWorkerCommand::Resize(viewport) => {
                Self::send_result(
                    backend.resize(viewport).and_then(|()| backend.snapshot()),
                    events,
                    wake_tx,
                );
            }
            BrowserWorkerCommand::Input(input) => {
                Self::send_result(
                    backend.input(input).and_then(|()| backend.snapshot()),
                    events,
                    wake_tx,
                );
            }
            BrowserWorkerCommand::Action(action) => {
                match backend.action(&action) {
                    Ok(value) => {
                        Self::send_event(BrowserWorkerEvent::ActionResult(value), events, wake_tx);
                    }
                    Err(error) => {
                        Self::send_event(
                            BrowserWorkerEvent::Error(error.to_string()),
                            events,
                            wake_tx,
                        );
                        return true;
                    }
                }
                Self::send_result(backend.snapshot(), events, wake_tx);
            }
            BrowserWorkerCommand::Snapshot => {
                Self::send_result(backend.snapshot(), events, wake_tx);
            }
            BrowserWorkerCommand::Shutdown => return false,
        }
        true
    }

    fn send_result(
        result: Result<BrowserBackendSnapshot, ah_web::WebError>,
        events: &mpsc::UnboundedSender<BrowserWorkerEvent>,
        wake_tx: Option<&mpsc::UnboundedSender<()>>,
    ) {
        let event = match result {
            Ok(snapshot) => BrowserWorkerEvent::Snapshot(snapshot),
            Err(error) => BrowserWorkerEvent::Error(error.to_string()),
        };
        Self::send_event(event, events, wake_tx);
    }

    fn send_event(
        event: BrowserWorkerEvent,
        events: &mpsc::UnboundedSender<BrowserWorkerEvent>,
        wake_tx: Option<&mpsc::UnboundedSender<()>>,
    ) {
        let _ = events.send(event);
        if let Some(wake_tx) = wake_tx {
            let _ = wake_tx.send(());
        }
    }
}

impl Drop for BrowserRuntime {
    fn drop(&mut self) {
        let _ = self.commands.send(BrowserWorkerCommand::Shutdown);
    }
}

#[derive(Clone, Debug)]
struct TerminalSessionView {
    session: Session,
    status: SharedString,
    ring: NotificationRing,
    blocks: Vec<BlockRow>,
    transcript: String,
}

impl TerminalSessionView {
    fn new(session: Session, status: SharedString) -> Self {
        Self {
            session,
            status,
            ring: NotificationRing::idle("ready"),
            blocks: Vec::new(),
            transcript: String::from("starting terminal...\n"),
        }
    }
}

#[derive(Clone, Debug)]
struct CommandCompletionMarker {
    begin_prefix: String,
    done_prefix: String,
    sequence: u64,
}

impl CommandCompletionMarker {
    fn new(sequence: u64) -> Self {
        Self {
            begin_prefix: format!("__AGENTHOUSE_BEGIN_{sequence}"),
            done_prefix: format!("__AGENTHOUSE_DONE_{sequence}:"),
            sequence,
        }
    }
}

#[derive(Clone, Debug)]
struct ActiveTerminalCommand {
    block_index: usize,
    marker: CommandCompletionMarker,
    command: String,
    began: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(dead_code)]
enum TerminalPromptSubmission {
    Command(String),
    Stdin(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SplitDirection {
    Right,
    Down,
}

impl fmt::Display for SplitDirection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Right => formatter.write_str("right"),
            Self::Down => formatter.write_str("down"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TerminalGridMetrics {
    cols: u16,
    rows: u16,
}

impl TerminalGridMetrics {
    fn new(cols: u16, rows: u16) -> Self {
        Self {
            cols: cols.max(1),
            rows: rows.max(1),
        }
    }
}

struct TerminalRuntime {
    terminal: Option<PtySession>,
    events: mpsc::UnboundedReceiver<TerminalEvent>,
    view: TerminalSessionView,
    emulator: TerminalEmulator,
    grid_metrics: TerminalGridMetrics,
    active_command: Option<ActiveTerminalCommand>,
    next_command_sequence: u64,
}

impl TerminalRuntime {
    fn spawn(name: impl Into<String>, cwd: PathBuf, wake_tx: mpsc::UnboundedSender<()>) -> Self {
        let name = name.into();
        let (events_tx, events) = mpsc::unbounded_channel();
        let mut session = Session::shell(name, cwd.clone());
        let terminal = Self::spawn_pty_for_session(session.id, &cwd, events_tx, wake_tx);
        let command = command_for_shell(&cwd);
        let terminal_cols = u16::try_from(TERMINAL_GRID_COLS).unwrap_or(120);
        let terminal_rows = u16::try_from(TERMINAL_GRID_ROWS).unwrap_or(36);
        let mut terminal = terminal.ok();
        if let Some(terminal) = terminal.as_mut() {
            let _ = terminal.resize(terminal_cols, terminal_rows);
            let _ = terminal.write(
                format!("stty cols {TERMINAL_GRID_COLS} rows {TERMINAL_GRID_ROWS}\n").as_bytes(),
            );
        }
        let status: SharedString = if terminal.is_some() {
            session.mark_running();
            format!("running {}", command.program).into()
        } else {
            "failed to start shell".into()
        };

        Self {
            terminal,
            events,
            view: TerminalSessionView::new(session, status),
            emulator: TerminalEmulator::new(TERMINAL_GRID_COLS, TERMINAL_GRID_ROWS),
            grid_metrics: TerminalGridMetrics::new(terminal_cols, terminal_rows),
            active_command: None,
            next_command_sequence: 1,
        }
    }

    fn spawn_pty_for_session(
        session_id: SessionId,
        cwd: &Path,
        events_tx: mpsc::UnboundedSender<TerminalEvent>,
        wake_tx: mpsc::UnboundedSender<()>,
    ) -> Result<PtySession, ah_terminal::TerminalError> {
        let command = command_for_shell(cwd);
        PtySession::spawn_with_wake(session_id, &command, events_tx, Some(wake_tx))
    }

    fn session_id(&self) -> SessionId {
        self.view.session.id
    }

    fn drain_events(&mut self) -> bool {
        let mut did_update = false;
        while let Ok(event) = self.events.try_recv() {
            did_update = true;
            self.handle_event(event);
        }
        did_update
    }

    fn handle_event(&mut self, event: TerminalEvent) {
        match event {
            TerminalEvent::Output { bytes, .. } => {
                self.emulator.advance(&bytes);
                let chunk = String::from_utf8_lossy(&bytes);
                self.view.transcript.push_str(&chunk);
                if let Some(active) = &self.active_command {
                    remove_marker_lines(&mut self.view.transcript, &active.marker);
                }
                retain_recent_utf8(&mut self.view.transcript, MAX_TERMINAL_OUTPUT_BYTES);
                self.append_active_output(&chunk);
            }
            TerminalEvent::Exited { code, .. } => {
                self.complete_active_block();
                self.view.session.mark_exited(code);
                self.set_status(match code {
                    Some(code) => format!("exited {code}"),
                    None => "exited".to_string(),
                });
                self.view.ring.update(RingState::Error, "terminal exited");
                self.terminal = None;
            }
        }
    }

    fn run_command(&mut self, command: String) -> Result<(), String> {
        if self.active_command.is_some() {
            self.set_status("command already running");
            self.view
                .ring
                .update(RingState::Running, "command already running");
            return Err("command already running".to_string());
        }

        let marker = CommandCompletionMarker::new(self.next_command_sequence);
        self.next_command_sequence += 1;
        let input = match terminal_input_for_command(&command, &marker) {
            Ok(input) => input,
            Err(error) => {
                self.set_status("script write failed");
                self.view
                    .ring
                    .update(RingState::Error, "script write failed");
                return Err(format!("failed to prepare command script: {error}"));
            }
        };
        let row = BlockRow {
            title: format!("$ {command}").into(),
            block: Block::new(self.view.session.id, Actor::Human, BlockKind::Command, ""),
        };
        self.view.blocks.insert(0, row);
        self.active_command = Some(ActiveTerminalCommand {
            block_index: 0,
            marker: marker.clone(),
            command: command.clone(),
            began: false,
        });

        match self
            .terminal
            .as_mut()
            .map(|terminal| terminal.write(input.as_bytes()))
        {
            Some(Ok(())) => {
                self.set_status(format!("running {command}"));
                self.view
                    .ring
                    .update(RingState::Running, "terminal command running");
                Ok(())
            }
            Some(Err(error)) => {
                self.finish_command_with_text(format!("failed to write command: {error}"));
                self.set_status("write failed");
                self.view.ring.update(RingState::Error, "write failed");
                Err(format!("failed to write command: {error}"))
            }
            None => {
                self.finish_command_with_text("terminal is not running");
                self.set_status("not running");
                self.view
                    .ring
                    .update(RingState::Error, "terminal not running");
                Err("terminal is not running".to_string())
            }
        }
    }

    fn write_input(&mut self, input: &str) -> Result<(), String> {
        self.write_input_inner(input, true)
    }

    fn write_input_from_ui(&mut self, input: &str) -> Result<(), String> {
        self.write_input_inner(input, false)
    }

    fn write_input_inner(&mut self, input: &str, audit_block: bool) -> Result<(), String> {
        match self
            .terminal
            .as_mut()
            .map(|terminal| terminal.write(input.as_bytes()))
        {
            Some(Ok(())) => {
                if audit_block {
                    let display = input.escape_debug().to_string();
                    self.append_system_block(
                        "Raw terminal input",
                        format!("wrote raw terminal input: {display}"),
                    );
                }
                self.set_status("input written");
                if audit_block {
                    self.view
                        .ring
                        .update(RingState::Running, "terminal input written");
                }
                Ok(())
            }
            Some(Err(error)) => {
                if audit_block {
                    self.append_system_block(
                        "Raw terminal input failed",
                        format!("failed to write raw terminal input: {error}"),
                    );
                }
                self.set_status("write failed");
                self.view.ring.update(RingState::Error, "write failed");
                Err(format!("failed to write terminal input: {error}"))
            }
            None => {
                if audit_block {
                    self.append_system_block(
                        "Raw terminal input failed",
                        "terminal is not running",
                    );
                }
                self.set_status("not running");
                self.view
                    .ring
                    .update(RingState::Error, "terminal not running");
                Err("terminal is not running".to_string())
            }
        }
    }

    fn interrupt(&mut self) -> Result<(), String> {
        self.interrupt_with_origin("AgentHouse-control")
    }

    #[allow(dead_code)]
    fn interrupt_from_ui(&mut self) -> Result<(), String> {
        self.interrupt_with_origin("AgentHouse UI")
    }

    fn interrupt_with_origin(&mut self, origin: &str) -> Result<(), String> {
        match self.terminal.as_mut().map(PtySession::interrupt) {
            Some(Ok(())) => {
                self.finish_command_with_text(format!("session interrupted by {origin}"));
                self.append_system_block("Session interrupted", "sent Ctrl-C to terminal session");
                self.set_status("interrupt sent");
                self.view
                    .ring
                    .update(RingState::Error, "session interrupted");
                Ok(())
            }
            Some(Err(error)) => {
                self.append_system_block(
                    "Session interrupt failed",
                    format!("failed to interrupt session: {error}"),
                );
                self.set_status("interrupt failed");
                self.view.ring.update(RingState::Error, "interrupt failed");
                Err(format!("failed to interrupt session: {error}"))
            }
            None => {
                self.append_system_block("Session interrupt failed", "terminal is not running");
                self.set_status("not running");
                self.view
                    .ring
                    .update(RingState::Error, "terminal not running");
                Err("terminal is not running".to_string())
            }
        }
    }

    #[allow(dead_code)]
    fn is_command_running(&self) -> bool {
        self.active_command.is_some()
    }

    fn terminate(&mut self) -> Result<(), String> {
        match self.terminal.as_mut().map(PtySession::terminate) {
            Some(Ok(())) => {
                self.finish_command_with_text("session terminated by AgentHouse-control");
                self.view.session.mark_exited(None);
                self.terminal = None;
                self.append_system_block("Session terminated", "terminated terminal child process");
                self.set_status("terminated");
                self.view
                    .ring
                    .update(RingState::Error, "session terminated");
                Ok(())
            }
            Some(Err(error)) => {
                self.append_system_block(
                    "Session terminate failed",
                    format!("failed to terminate session: {error}"),
                );
                self.set_status("terminate failed");
                self.view.ring.update(RingState::Error, "terminate failed");
                Err(format!("failed to terminate session: {error}"))
            }
            None => {
                self.append_system_block(
                    "Session terminate skipped",
                    "terminal is already stopped",
                );
                self.view.session.mark_exited(None);
                self.set_status("terminated");
                self.view
                    .ring
                    .update(RingState::Error, "session already stopped");
                Ok(())
            }
        }
    }

    fn restart(&mut self, wake_tx: mpsc::UnboundedSender<()>) -> Result<(), String> {
        if let Some(terminal) = self.terminal.as_mut() {
            let _ = terminal.terminate();
        }
        self.finish_command_with_text("session restarted by AgentHouse-control");

        let (events_tx, events) = mpsc::unbounded_channel();
        match Self::spawn_pty_for_session(
            self.view.session.id,
            &self.view.session.cwd,
            events_tx,
            wake_tx,
        ) {
            Ok(mut terminal) => {
                let _ = terminal.resize(
                    u16::try_from(TERMINAL_GRID_COLS).unwrap_or(120),
                    u16::try_from(TERMINAL_GRID_ROWS).unwrap_or(36),
                );
                let _ = terminal.write(
                    format!("stty cols {TERMINAL_GRID_COLS} rows {TERMINAL_GRID_ROWS}\n")
                        .as_bytes(),
                );
                let process_id = terminal.process_id();
                self.terminal = Some(terminal);
                self.events = events;
                self.emulator = TerminalEmulator::new(TERMINAL_GRID_COLS, TERMINAL_GRID_ROWS);
                self.grid_metrics = TerminalGridMetrics::new(
                    u16::try_from(TERMINAL_GRID_COLS).unwrap_or(120),
                    u16::try_from(TERMINAL_GRID_ROWS).unwrap_or(36),
                );
                self.view.session.mark_running();
                self.append_system_block(
                    "Session restarted",
                    match process_id {
                        Some(process_id) => {
                            format!("restarted terminal session with pid {process_id}")
                        }
                        None => "restarted terminal session".to_string(),
                    },
                );
                self.set_status("restarted");
                self.view
                    .ring
                    .update(RingState::Complete, "session restarted");
                Ok(())
            }
            Err(error) => {
                self.terminal = None;
                self.events = events;
                self.view.session.mark_exited(None);
                self.append_system_block(
                    "Session restart failed",
                    format!("failed to restart terminal session: {error}"),
                );
                self.set_status("restart failed");
                self.view.ring.update(RingState::Error, "restart failed");
                Err(format!("failed to restart session: {error}"))
            }
        }
    }

    fn resize(&mut self, cols: u16, rows: u16) -> Result<(), String> {
        if cols == 0 || rows == 0 {
            return Err("terminal size must be non-zero".to_string());
        }

        match self
            .terminal
            .as_ref()
            .map(|terminal| terminal.resize(cols, rows))
        {
            Some(Ok(())) => {
                self.emulator.resize(cols as usize, rows as usize);
                self.grid_metrics = TerminalGridMetrics::new(cols, rows);
                self.append_system_block(
                    "Terminal resized",
                    format!("resized terminal to {cols} cols x {rows} rows"),
                );
                self.set_status(format!("resized {cols}x{rows}"));
                self.view
                    .ring
                    .update(RingState::Complete, "terminal resized");
                Ok(())
            }
            Some(Err(error)) => {
                self.append_system_block(
                    "Terminal resize failed",
                    format!("failed to resize terminal: {error}"),
                );
                self.set_status("resize failed");
                self.view.ring.update(RingState::Error, "resize failed");
                Err(format!("failed to resize terminal: {error}"))
            }
            None => {
                self.append_system_block("Terminal resize failed", "terminal is not running");
                self.set_status("not running");
                self.view
                    .ring
                    .update(RingState::Error, "terminal not running");
                Err("terminal is not running".to_string())
            }
        }
    }

    fn sync_measured_size(&mut self, metrics: TerminalGridMetrics) -> Result<bool, String> {
        if self.grid_metrics == metrics {
            return Ok(false);
        }

        match self
            .terminal
            .as_ref()
            .map(|terminal| terminal.resize(metrics.cols, metrics.rows))
        {
            Some(Ok(())) => {
                self.emulator
                    .resize(metrics.cols as usize, metrics.rows as usize);
                self.grid_metrics = metrics;
                self.set_status(format!("resized {}x{}", metrics.cols, metrics.rows));
                Ok(true)
            }
            Some(Err(error)) => {
                self.set_status("resize failed");
                self.view.ring.update(RingState::Error, "resize failed");
                Err(format!("failed to resize terminal: {error}"))
            }
            None => Err("terminal is not running".to_string()),
        }
    }

    fn append_active_output(&mut self, chunk: &str) {
        let Some(active) = self.active_command.clone() else {
            return;
        };

        let mut exit_code = None;
        let mut began = active.began;
        if let Some(row) = self.view.blocks.get_mut(active.block_index) {
            row.block.text.push_str(chunk);
            if !began {
                began = discard_until_begin_marker(&mut row.block.text, &active.marker);
            }

            if began {
                exit_code = extract_completion_exit_code(&mut row.block.text, &active.marker);
                remove_marker_lines(&mut row.block.text, &active.marker);
                sanitize_terminal_block_text(&mut row.block.text);
                remove_echoed_command_lines(&mut row.block.text, &active.command);
                retain_recent_utf8(&mut row.block.text, MAX_BLOCK_OUTPUT_BYTES);
            } else {
                retain_recent_utf8(&mut row.block.text, MAX_BLOCK_OUTPUT_BYTES);
            }

            if exit_code.is_some() {
                finalize_command_block_text(&mut row.block, &active.command);
                row.block.complete();
            }
        }

        if let Some(code) = exit_code {
            self.active_command = None;
            self.set_status(format!("last exit {code}"));
            let ring_state = if code == 0 {
                RingState::Complete
            } else {
                RingState::Error
            };
            self.view
                .ring
                .update(ring_state, format!("terminal command exited {code}"));
        } else if began && let Some(active) = self.active_command.as_mut() {
            active.began = true;
        }
    }

    fn complete_active_block(&mut self) {
        if let Some(active) = self.active_command.take()
            && let Some(row) = self.view.blocks.get_mut(active.block_index)
        {
            if row.block.text.trim().is_empty() {
                row.block.text = "command sent; no terminal output captured yet".to_string();
            } else {
                row.block
                    .text
                    .push_str("\nterminal exited before completion marker");
            }
            remove_marker_lines(&mut row.block.text, &active.marker);
            sanitize_terminal_block_text(&mut row.block.text);
            finalize_command_block_text(&mut row.block, &active.command);
            row.block.complete();
        }
    }

    fn finish_command_with_text(&mut self, text: impl Into<String>) {
        if let Some(active) = self.active_command.take()
            && let Some(row) = self.view.blocks.get_mut(active.block_index)
        {
            row.block.text = text.into();
            sanitize_terminal_block_text(&mut row.block.text);
            row.block.complete();
        }
    }

    fn append_system_block(&mut self, title: impl Into<SharedString>, text: impl Into<String>) {
        if let Some(active) = self.active_command.as_mut() {
            active.block_index += 1;
        }
        self.view.blocks.insert(
            0,
            completed_block(
                self.view.session.id,
                Actor::System,
                BlockKind::System,
                title,
                text,
            ),
        );
    }

    fn set_status(&mut self, status: impl Into<SharedString>) {
        self.view.status = status.into();
    }
}

pub struct AgentHouseShell {
    workspaces: Vec<Workspace>,
    active_workspace_index: usize,
    ui_language: UiLanguage,
    ui_theme_scheme: UiThemeSchemePreference,
    terminals: Vec<TerminalRuntime>,
    store: Option<Store>,
    control_requests: mpsc::UnboundedReceiver<QueuedControlRequest>,
    events: Vec<ControlEvent>,
    next_event_sequence: u64,
    terminal_command_inputs: HashMap<SessionId, String>,
    terminal_marked_text: HashMap<SessionId, String>,
    browser_address_inputs: HashMap<SessionId, String>,
    browser_address_edits: HashMap<SessionId, BrowserAddressEditState>,
    browsers: Vec<BrowserRuntime>,
    terminal_focus_handles: HashMap<SessionId, FocusHandle>,
    browser_address_focus_handles: HashMap<SessionId, FocusHandle>,
    browser_content_focus_handles: HashMap<SessionId, FocusHandle>,
    browser_content_bounds: HashMap<SessionId, Bounds<Pixels>>,
    #[allow(dead_code)]
    terminal_command_focus: FocusHandle,
    terminal_wake_tx: mpsc::UnboundedSender<()>,
    browser_wake_tx: mpsc::UnboundedSender<()>,
}

impl AgentHouseShell {
    #[must_use]
    pub fn new(
        cx: &mut Context<Self>,
        control_requests: mpsc::UnboundedReceiver<QueuedControlRequest>,
        store_path: PathBuf,
    ) -> Self {
        let store = match Store::open(&store_path) {
            Ok(store) => Some(store),
            Err(error) => {
                tracing::warn!(?error, path = %store_path.display(), "failed to open AgentHouse store");
                None
            }
        };
        let (terminal_wake_tx, mut terminal_wake_rx) = mpsc::unbounded_channel();
        let (browser_wake_tx, mut browser_wake_rx) = mpsc::unbounded_channel();

        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(50))
                    .await;
                let did_update = this
                    .update(cx, |this, cx| {
                        let did_update = this.drain_control_requests(cx);
                        if did_update {
                            cx.notify();
                            true
                        } else {
                            false
                        }
                    })
                    .unwrap_or(false);

                if !did_update {
                    continue;
                }
            }
        })
        .detach();

        cx.spawn(async move |this, cx| {
            while browser_wake_rx.recv().await.is_some() {
                cx.background_executor()
                    .timer(Duration::from_millis(8))
                    .await;
                let did_update = this
                    .update(cx, |this, cx| {
                        while browser_wake_rx.try_recv().is_ok() {}
                        let did_update = this.drain_browser_events();
                        if did_update {
                            cx.notify();
                        }
                        did_update
                    })
                    .unwrap_or(false);
                if !did_update {
                    continue;
                }
            }
        })
        .detach();

        cx.spawn(async move |this, cx| {
            while terminal_wake_rx.recv().await.is_some() {
                cx.background_executor()
                    .timer(Duration::from_millis(8))
                    .await;
                let did_update = this
                    .update(cx, |this, cx| {
                        while terminal_wake_rx.try_recv().is_ok() {}
                        let did_update = this.drain_terminal_events();
                        if did_update {
                            cx.notify();
                        }
                        did_update
                    })
                    .unwrap_or(false);
                if !did_update {
                    continue;
                }
            }
        })
        .detach();

        let mut shell = Self {
            workspaces: Vec::new(),
            active_workspace_index: 0,
            ui_language: UiLanguage::from_environment(),
            ui_theme_scheme: DEFAULT_UI_THEME_SCHEME,
            terminals: Vec::new(),
            store,
            control_requests,
            events: vec![ControlEvent {
                sequence: 1,
                level: "info".to_string(),
                topic: "app".to_string(),
                message: "AgentHouse shell initialized".to_string(),
            }],
            next_event_sequence: 2,
            terminal_command_inputs: HashMap::new(),
            terminal_marked_text: HashMap::new(),
            browser_address_inputs: HashMap::new(),
            browser_address_edits: HashMap::new(),
            browsers: Vec::new(),
            terminal_focus_handles: HashMap::new(),
            browser_address_focus_handles: HashMap::new(),
            browser_content_focus_handles: HashMap::new(),
            browser_content_bounds: HashMap::new(),
            terminal_command_focus: cx.focus_handle().tab_stop(true),
            terminal_wake_tx,
            browser_wake_tx,
        };
        shell.restore_from_store();
        shell.refresh_settings_menu(cx);
        if !shell.workspaces.is_empty() {
            shell.ensure_workspaces_have_terminal_panes();
            shell.ensure_browser_runtimes_for_workspaces();
            shell.ensure_terminal_focus_handles(cx);
            shell.ensure_browser_focus_handles(cx);
        }
        shell.persist_state();
        shell
    }

    fn active_workspace(&self) -> &Workspace {
        &self.workspaces[self.active_workspace_index]
    }

    fn active_workspace_mut(&mut self) -> &mut Workspace {
        &mut self.workspaces[self.active_workspace_index]
    }

    fn restore_from_store(&mut self) {
        let Some(store) = &self.store else {
            return;
        };
        let Ok(workspaces) = store.load_workspaces() else {
            tracing::warn!("failed to load workspaces from store");
            return;
        };
        if workspaces.is_empty() {
            self.restore_control_state();
            return;
        }

        let referenced_session_ids = terminal_session_ids_for_workspaces(&workspaces);
        let sessions = match store.load_sessions() {
            Ok(sessions) => sessions,
            Err(error) => {
                tracing::warn!(?error, "failed to load sessions from store");
                Vec::new()
            }
        };
        let mut terminals = Vec::new();
        for session in sessions {
            if !referenced_session_ids.contains(&session.id) {
                continue;
            }
            let cwd = session.cwd.clone();
            let name = session.name.clone();
            let mut runtime = TerminalRuntime::spawn(name, cwd, self.terminal_wake_tx.clone());
            runtime.view.session = session;
            runtime.view.blocks = store
                .load_blocks_for_session(&runtime.view.session)
                .unwrap_or_default()
                .into_iter()
                .map(|block| BlockRow {
                    title: block_title_for_restore(&block),
                    block,
                })
                .collect();
            runtime.view.ring = NotificationRing::idle("restored");
            terminals.push(runtime);
        }

        self.workspaces = workspaces;
        self.active_workspace_index = 0;
        if !terminals.is_empty() {
            self.terminals = terminals;
        }
        self.restore_control_state();
        self.push_event("info", "store", "restored persisted workspace state");
    }

    fn restore_control_state(&mut self) {
        let Some(store) = &self.store else {
            return;
        };
        let state = match store.load_control_state() {
            Ok(Some(state)) => state,
            Ok(None) => return,
            Err(error) => {
                tracing::warn!(?error, "failed to load control state from store");
                return;
            }
        };

        if let Some(active_workspace_id) = state.active_workspace_id
            && let Some(index) = self.workspace_index_by_id(active_workspace_id)
        {
            self.active_workspace_index = index;
        }
        if let Some(language) = state.ui_language {
            self.ui_language = UiLanguage::from_preference(language);
        }
        if let Some(scheme) = state
            .ui_theme_scheme
            .as_deref()
            .and_then(UiThemeSchemePreference::from_control_code)
        {
            self.ui_theme_scheme = scheme;
        }
        self.events = state.events;
        self.next_event_sequence = state.next_event_sequence.max(
            self.events
                .iter()
                .map(|event| event.sequence.saturating_add(1))
                .max()
                .unwrap_or(1),
        );
        for ring in state.session_rings {
            let Some(terminal) = self.terminal_by_session_mut(ring.session_id) else {
                continue;
            };
            terminal.view.ring = NotificationRing::from_persisted(ring);
        }
    }

    fn persist_state(&self) {
        let Some(store) = &self.store else {
            return;
        };
        for workspace in &self.workspaces {
            if let Err(error) = store.save_workspace(workspace) {
                tracing::warn!(?error, "failed to persist workspace");
            }
        }
        for terminal in &self.terminals {
            if let Err(error) = store.save_session(&terminal.view.session) {
                tracing::warn!(?error, "failed to persist session");
            }
            for row in &terminal.view.blocks {
                if let Err(error) = store.save_block(&row.block) {
                    tracing::warn!(?error, "failed to persist block");
                }
            }
        }
        let control_state = self.persisted_control_state();
        if let Err(error) = store.save_control_state(&control_state) {
            tracing::warn!(?error, "failed to persist control state");
        }
    }

    fn activate_workspace(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.workspaces.len() {
            self.active_workspace_index = index;
            let name = self.workspaces[index].name.clone();
            self.push_event("info", "workspace", format!("activated workspace {name}"));
            self.persist_state();
            cx.notify();
        }
    }

    fn ui_text(&self, zh_cn: &'static str, en: &'static str) -> &'static str {
        self.ui_language.select(zh_cn, en)
    }

    fn app_settings_summary(&self) -> AppSettingsSummary {
        AppSettingsSummary::new(
            self.ui_language.preference(),
            self.ui_theme_scheme,
            FIXED_UI_THEME_MODE,
        )
    }

    fn set_ui_language(&mut self, language: UiLanguagePreference) -> ControlResult {
        self.ui_language = UiLanguage::from_preference(language);
        let settings = self.app_settings_summary();
        self.persist_state();
        self.push_event(
            "info",
            "settings",
            format!("set UI language to {:?}", settings.ui_language),
        );
        ControlResult::UiLanguageSet { settings }
    }

    fn set_ui_theme_scheme(&mut self, scheme: UiThemeSchemePreference) -> ControlResult {
        self.ui_theme_scheme = scheme;
        let settings = self.app_settings_summary();
        self.persist_state();
        self.push_event(
            "info",
            "settings",
            format!("set UI color scheme to {}", settings.ui_theme_scheme),
        );
        ControlResult::UiThemeSchemeSet { settings }
    }

    fn refresh_settings_menu(&self, cx: &mut Context<Self>) {
        refresh_app_menu(cx, self.ui_theme_scheme, self.ui_language.preference());
    }

    fn set_ui_language_from_ui(&mut self, language: UiLanguage, cx: &mut Context<Self>) {
        if self.ui_language == language {
            return;
        }
        let _ = self.set_ui_language(language.preference());
        self.refresh_settings_menu(cx);
        cx.notify();
    }

    fn set_ui_theme_scheme_from_ui(
        &mut self,
        scheme: UiThemeSchemePreference,
        cx: &mut Context<Self>,
    ) {
        if self.ui_theme_scheme == scheme {
            return;
        }
        let _ = self.set_ui_theme_scheme(scheme);
        self.refresh_settings_menu(cx);
        cx.notify();
    }

    fn close_workspace_from_ui(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.workspaces.len() {
            return;
        }

        let workspace = self.workspaces.remove(index);
        let workspace_id = workspace.id;
        let session_ids = terminal_session_ids_for_workspace(&workspace);
        let browser_session_ids = browser_session_ids_for_workspace(&workspace);
        for session_id in session_ids {
            self.remove_terminal_runtime(session_id);
        }
        for session_id in browser_session_ids {
            self.remove_browser_runtime(session_id);
        }
        if let Some(store) = &self.store
            && let Err(error) = store.delete_workspace(workspace_id)
        {
            tracing::warn!(?error, "failed to delete workspace from store");
        }

        if self.workspaces.is_empty() {
            self.active_workspace_index = 0;
        } else if self.active_workspace_index > index {
            self.active_workspace_index -= 1;
        } else {
            self.active_workspace_index = self
                .active_workspace_index
                .min(self.workspaces.len().saturating_sub(1));
        }

        self.ensure_workspaces_have_terminal_panes();
        self.ensure_terminal_focus_handles(cx);
        self.ensure_browser_focus_handles(cx);
        self.push_event(
            "info",
            "workspace",
            format!("closed workspace {workspace_id:?}"),
        );
        self.persist_state();
        cx.notify();
    }

    fn activate_window(&mut self, window_id: WindowId, cx: &mut Context<Self>) {
        self.active_workspace_mut().active_window_id = Some(window_id);
        self.push_event("info", "window", format!("activated window {window_id:?}"));
        self.persist_state();
        cx.notify();
    }

    fn focus_active_terminal_in_window(
        &mut self,
        window_id: WindowId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace_window) = self.window_by_id(window_id) else {
            return;
        };
        let Some(session_id) = workspace_window
            .active_tab()
            .and_then(terminal_session_id_for_tab)
        else {
            return;
        };
        self.focus_terminal_session(session_id, window, cx);
    }

    fn focus_active_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(window_id) = self.active_workspace().active_window_id else {
            return;
        };
        self.focus_active_terminal_in_window(window_id, window, cx);
    }

    fn focus_active_tab_in_window(
        &mut self,
        window_id: WindowId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace_window) = self.window_by_id(window_id) else {
            return;
        };
        match workspace_window.active_tab() {
            Some(tab) if terminal_session_id_for_tab(tab).is_some() => {
                let session_id = terminal_session_id_for_tab(tab).expect("checked terminal tab");
                self.focus_terminal_session(session_id, window, cx);
            }
            Some(tab) if browser_session_id_for_tab(tab).is_some() => {
                let session_id = browser_session_id_for_tab(tab).expect("checked browser tab");
                self.focus_browser_content(session_id, window, cx);
            }
            _ => {}
        }
    }

    fn focus_active_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(window_id) = self.active_workspace().active_window_id else {
            return;
        };
        self.focus_active_tab_in_window(window_id, window, cx);
    }

    fn focus_terminal_session(
        &mut self,
        session_id: SessionId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ensure_terminal_focus_handles(cx);
        if let Some(focus_handle) = self.terminal_focus_handles.get(&session_id) {
            window.focus(focus_handle, cx);
        }
    }

    fn focus_browser_content(
        &mut self,
        session_id: SessionId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ensure_browser_focus_handles(cx);
        if let Some(focus_handle) = self.browser_content_focus_handles.get(&session_id) {
            window.focus(focus_handle, cx);
        }
    }

    fn ensure_terminal_focus_handles(&mut self, cx: &mut Context<Self>) {
        let session_ids = self
            .terminals
            .iter()
            .map(TerminalRuntime::session_id)
            .collect::<HashSet<_>>();
        self.terminal_focus_handles
            .retain(|session_id, _| session_ids.contains(session_id));
        for session_id in session_ids {
            self.terminal_focus_handles
                .entry(session_id)
                .or_insert_with(|| cx.focus_handle().tab_stop(true));
        }
    }

    fn open_workspace_folder_from_ui(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open Folder".into()),
        });

        cx.spawn_in(window, async move |this, cx| {
            let selected = match paths.await {
                Ok(Ok(Some(mut paths))) => paths.pop(),
                Ok(Ok(None)) => None,
                Ok(Err(error)) => {
                    let _ = this.update(cx, |this, _cx| {
                        this.push_event(
                            "warn",
                            "workspace",
                            format!("failed to open workspace folder picker: {error}"),
                        );
                    });
                    None
                }
                Err(_) => {
                    let _ = this.update(cx, |this, _cx| {
                        this.push_event(
                            "warn",
                            "workspace",
                            "workspace folder picker was canceled before returning",
                        );
                    });
                    None
                }
            };

            let Some(root) = selected else {
                return;
            };

            let _ = this.update_in(cx, |this, window, cx| {
                if let Some(window_id) = this.create_workspace_for_root(root, cx) {
                    this.focus_active_terminal_in_window(window_id, window, cx);
                }
            });
        })
        .detach();
    }

    fn create_workspace_for_root(
        &mut self,
        root: PathBuf,
        cx: &mut Context<Self>,
    ) -> Option<WindowId> {
        let root = canonical_workspace_root(root);
        if let Some(index) = self
            .workspaces
            .iter()
            .position(|workspace| workspace.root.as_ref() == Some(&root))
        {
            self.active_workspace_index = index;
            self.ensure_workspaces_have_terminal_panes();
            self.ensure_terminal_focus_handles(cx);
            self.ensure_browser_focus_handles(cx);
            let name = self.workspaces[index].name.clone();
            self.push_event("info", "workspace", format!("activated workspace {name}"));
            self.persist_state();
            cx.notify();
            return self.workspaces[index].active_window_id;
        }

        let workspace = workspace_for_root(self.workspaces.len(), root.clone());
        let name = workspace.name.clone();
        let workspace_id = workspace.id;
        self.workspaces.push(workspace);
        self.active_workspace_index = self.workspaces.len() - 1;
        let window_id = self.add_terminal_pane_to_workspace(self.active_workspace_index);
        self.push_event(
            "info",
            "workspace",
            format!(
                "created workspace {workspace_id:?} ({name}) at {}",
                root.display()
            ),
        );
        self.ensure_terminal_focus_handles(cx);
        self.ensure_browser_focus_handles(cx);
        self.persist_state();
        cx.notify();
        window_id
    }

    #[allow(dead_code)]
    fn create_terminal_pane_from_ui(&mut self, cx: &mut Context<Self>) -> Option<WindowId> {
        let workspace_index = self.active_workspace_index;
        let window_id = self.add_terminal_pane_to_workspace(workspace_index);
        if let Some(window_id) = window_id {
            self.push_event(
                "info",
                "window",
                format!("created terminal pane {window_id:?}"),
            );
            self.ensure_terminal_focus_handles(cx);
        } else {
            self.push_event("warn", "window", "workspace pane limit reached");
        }
        self.persist_state();
        cx.notify();
        window_id
    }

    fn open_terminal_in_window_from_ui(
        &mut self,
        window_id: WindowId,
        cx: &mut Context<Self>,
    ) -> Option<SessionId> {
        let Some(session_id) = self.add_terminal_tab_to_window(window_id, "Terminal") else {
            self.push_event("error", "window", "cannot open terminal: window not found");
            cx.notify();
            return None;
        };
        self.push_event(
            "info",
            "terminal",
            format!("opened terminal tab for session {session_id:?} in window {window_id:?}"),
        );
        self.ensure_terminal_focus_handles(cx);
        self.persist_state();
        cx.notify();
        Some(session_id)
    }

    fn open_browser_in_window_from_ui(
        &mut self,
        window_id: WindowId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace_index) = self.workspace_index_for_window(window_id) else {
            self.push_event("error", "window", "cannot open browser: window not found");
            cx.notify();
            return;
        };

        let url = DEFAULT_BROWSER_URL.to_string();
        let browser = match BrowserRuntime::new_native(
            "Browser",
            url.clone(),
            self.browser_wake_tx.clone(),
        ) {
            Ok(browser) => browser,
            Err(error) => {
                self.push_event("error", "web", error);
                cx.notify();
                return;
            }
        };
        let session_id = browser.session_id();
        let Some(workspace_window) = self.workspaces[workspace_index]
            .windows
            .iter_mut()
            .find(|workspace_window| workspace_window.id == window_id)
        else {
            self.push_event("error", "window", "cannot open browser: window not found");
            cx.notify();
            return;
        };

        workspace_window.push_tab(WindowTab::web_with_session(
            "Browser",
            session_id,
            url.clone(),
        ));
        self.workspaces[workspace_index].active_window_id = Some(window_id);
        self.active_workspace_index = workspace_index;
        self.browsers.push(browser);
        self.refresh_browser_session(session_id);
        self.ensure_browser_focus_handles(cx);
        self.focus_browser_address_for_edit(session_id, Some(url));
        if let Some(focus_handle) = self.browser_address_focus_handles.get(&session_id) {
            window.focus(focus_handle, cx);
        }
        self.push_event("info", "window", "opened browser tab");
        self.persist_state();
        cx.notify();
    }

    fn split_window_from_ui(
        &mut self,
        window_id: WindowId,
        direction: SplitDirection,
        cx: &mut Context<Self>,
    ) -> Option<WindowId> {
        let Some(workspace_index) = self.workspace_index_for_window(window_id) else {
            self.push_event("error", "window", "cannot split: window not found");
            cx.notify();
            return None;
        };
        let workspace_id = self.workspaces[workspace_index].id;
        self.workspaces[workspace_index].layout.mode = match direction {
            SplitDirection::Right => LayoutMode::Columns,
            SplitDirection::Down => LayoutMode::Grid,
        };
        let new_window_id = self.add_terminal_pane_to_workspace(workspace_index);
        match new_window_id {
            Some(new_window_id) => {
                self.ensure_terminal_focus_handles(cx);
                self.push_event(
                    "info",
                    "window",
                    format!(
                        "split {direction} from {window_id:?}; created terminal pane {new_window_id:?} in workspace {workspace_id:?}"
                    ),
                );
            }
            None => self.push_event("warn", "window", "workspace pane limit reached"),
        }
        self.persist_state();
        cx.notify();
        new_window_id
    }

    fn active_window_id_for_menu(&self) -> Option<WindowId> {
        self.workspaces
            .get(self.active_workspace_index)
            .and_then(|workspace| workspace.active_window_id)
            .or_else(|| {
                self.workspaces
                    .get(self.active_workspace_index)
                    .and_then(|workspace| workspace.windows.first().map(|window| window.id))
            })
    }

    fn open_terminal_from_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(window_id) = self.active_window_id_for_menu() else {
            self.push_event("warn", "window", "cannot open terminal: no active window");
            cx.notify();
            return;
        };
        if let Some(session_id) = self.open_terminal_in_window_from_ui(window_id, cx) {
            self.focus_terminal_session(session_id, window, cx);
        }
    }

    fn open_browser_from_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(window_id) = self.active_window_id_for_menu() else {
            self.push_event("warn", "window", "cannot open browser: no active window");
            cx.notify();
            return;
        };
        self.open_browser_in_window_from_ui(window_id, window, cx);
    }

    fn split_window_from_menu(
        &mut self,
        direction: SplitDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(window_id) = self.active_window_id_for_menu() else {
            self.push_event("warn", "window", "cannot split: no active window");
            cx.notify();
            return;
        };
        if let Some(new_window_id) = self.split_window_from_ui(window_id, direction, cx) {
            self.focus_active_tab_in_window(new_window_id, window, cx);
        }
    }

    fn close_active_workspace_from_menu(&mut self, cx: &mut Context<Self>) {
        if self.workspaces.is_empty() {
            return;
        }
        self.close_workspace_from_ui(self.active_workspace_index, cx);
    }

    fn begin_active_workspace_rename_from_menu(&mut self, cx: &mut Context<Self>) {
        self.push_event(
            "info",
            "settings",
            "workspace rename is reserved for a later Alpha settings pass",
        );
        cx.notify();
    }

    fn ensure_workspaces_have_terminal_panes(&mut self) {
        if self.workspaces.is_empty() {
            return;
        }

        let active_workspace_index = self
            .active_workspace_index
            .min(self.workspaces.len().saturating_sub(1));
        for workspace_index in 0..self.workspaces.len() {
            if self.workspaces[workspace_index].windows.is_empty() {
                let _ = self.add_terminal_pane_to_workspace(workspace_index);
                continue;
            }

            let empty_window_ids = self.workspaces[workspace_index]
                .windows
                .iter()
                .filter(|window| window.tabs.is_empty())
                .map(|window| window.id)
                .collect::<Vec<_>>();
            for window_id in empty_window_ids {
                let _ = self.add_terminal_tab_to_window(window_id, "Terminal");
            }
        }
        self.active_workspace_index = active_workspace_index;
    }

    fn add_terminal_pane_to_workspace(&mut self, workspace_index: usize) -> Option<WindowId> {
        if workspace_index >= self.workspaces.len()
            || self.workspaces[workspace_index].windows.len() >= MAX_WORKSPACE_PANES
        {
            return None;
        }

        let window = workspace_window_for_ui(self.workspaces[workspace_index].windows.len());
        let window_id = window.id;
        self.workspaces[workspace_index].push_window(window);
        self.active_workspace_index = workspace_index;
        self.add_terminal_tab_to_window(window_id, "Terminal")?;
        Some(window_id)
    }

    fn add_terminal_tab_to_window(
        &mut self,
        window_id: WindowId,
        title: impl Into<String>,
    ) -> Option<SessionId> {
        self.add_terminal_tab_to_window_with_cwd(window_id, title, None)
    }

    fn add_terminal_tab_to_window_with_cwd(
        &mut self,
        window_id: WindowId,
        title: impl Into<String>,
        cwd: Option<PathBuf>,
    ) -> Option<SessionId> {
        let workspace_index = self.workspace_index_for_window(window_id)?;
        let cwd = cwd
            .or_else(|| self.workspaces[workspace_index].root.clone())
            .unwrap_or_else(Self::default_terminal_cwd);
        let title = title.into();
        let terminal = TerminalRuntime::spawn(title.clone(), cwd, self.terminal_wake_tx.clone());
        let session_id = terminal.session_id();

        let workspace_window = self.workspaces[workspace_index]
            .windows
            .iter_mut()
            .find(|workspace_window| workspace_window.id == window_id)?;

        workspace_window.push_tab(WindowTab::terminal(title, session_id));
        self.workspaces[workspace_index].active_window_id = Some(window_id);
        self.active_workspace_index = workspace_index;
        self.terminals.push(terminal);
        Some(session_id)
    }

    #[allow(dead_code)]
    fn close_window_from_ui(&mut self, window_id: WindowId, cx: &mut Context<Self>) {
        let _ = self.close_window(window_id);
        self.ensure_workspaces_have_terminal_panes();
        self.ensure_terminal_focus_handles(cx);
        self.ensure_browser_focus_handles(cx);
        self.persist_state();
        cx.notify();
    }

    fn close_window_tab_from_ui(
        &mut self,
        window_id: WindowId,
        tab_id: TabId,
        cx: &mut Context<Self>,
    ) {
        let close_window = self
            .window_by_id(window_id)
            .is_some_and(|workspace_window| workspace_window.tabs.len() <= 1);
        if close_window {
            let _ = self.close_window(window_id);
        } else {
            let _ = self.close_window_tab(window_id, tab_id);
        }
        self.ensure_workspaces_have_terminal_panes();
        self.ensure_terminal_focus_handles(cx);
        self.ensure_browser_focus_handles(cx);
        self.persist_state();
        cx.notify();
    }

    fn remove_terminal_runtime(&mut self, session_id: SessionId) {
        if let Some(index) = self
            .terminals
            .iter()
            .position(|terminal| terminal.session_id() == session_id)
        {
            let mut terminal = self.terminals.remove(index);
            let _ = terminal.terminate();
        }
        self.terminal_focus_handles.remove(&session_id);
        self.terminal_command_inputs.remove(&session_id);
        self.terminal_marked_text.remove(&session_id);
        if let Some(store) = &self.store
            && let Err(error) = store.delete_session(session_id)
        {
            tracing::warn!(?error, "failed to delete session from store");
        }
    }

    fn remove_browser_runtime(&mut self, session_id: SessionId) {
        self.browsers
            .retain(|browser| browser.session_id() != session_id);
        self.browser_address_inputs.remove(&session_id);
        self.browser_address_edits.remove(&session_id);
        self.browser_address_focus_handles.remove(&session_id);
        self.browser_content_focus_handles.remove(&session_id);
        self.browser_content_bounds.remove(&session_id);
    }

    fn browser_by_session(&self, session_id: SessionId) -> Option<&BrowserRuntime> {
        self.browsers
            .iter()
            .find(|browser| browser.session_id() == session_id)
    }

    fn browser_by_session_mut(&mut self, session_id: SessionId) -> Option<&mut BrowserRuntime> {
        self.browsers
            .iter_mut()
            .find(|browser| browser.session_id() == session_id)
    }

    fn browser_session_ids_for_workspace(
        &self,
        workspace_id: Option<WorkspaceId>,
    ) -> HashSet<SessionId> {
        let Some(workspace) = workspace_id
            .and_then(|workspace_id| self.workspace_by_id(workspace_id))
            .or_else(|| self.workspaces.get(self.active_workspace_index))
        else {
            return HashSet::new();
        };
        browser_session_ids_for_workspace(workspace)
            .into_iter()
            .collect()
    }

    fn ensure_browser_runtimes_for_workspaces(&mut self) {
        let tabs = self
            .workspaces
            .iter()
            .flat_map(|workspace| &workspace.windows)
            .flat_map(|window| &window.tabs)
            .filter_map(|tab| match &tab.content {
                WindowContent::Web { session_id, url } => {
                    Some((*session_id, tab.title.clone(), url.clone()))
                }
                WindowContent::Terminal { .. } | WindowContent::FilePreview { .. } => None,
            })
            .collect::<Vec<_>>();
        let referenced = tabs
            .iter()
            .map(|(session_id, _, _)| *session_id)
            .collect::<HashSet<_>>();
        self.browsers
            .retain(|browser| referenced.contains(&browser.session_id()));
        for (session_id, title, url) in tabs {
            if self.browser_by_session(session_id).is_some() {
                continue;
            }
            let mut runtime =
                match BrowserRuntime::new_native(title, url, self.browser_wake_tx.clone()) {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        self.push_event("error", "web", error);
                        continue;
                    }
                };
            runtime.state.id = session_id;
            self.browsers.push(runtime);
        }
    }

    fn ensure_browser_focus_handles(&mut self, cx: &mut Context<Self>) {
        let session_ids = self
            .browsers
            .iter()
            .map(BrowserRuntime::session_id)
            .collect::<HashSet<_>>();
        self.browser_address_inputs
            .retain(|session_id, _| session_ids.contains(session_id));
        self.browser_address_edits
            .retain(|session_id, _| session_ids.contains(session_id));
        self.browser_address_focus_handles
            .retain(|session_id, _| session_ids.contains(session_id));
        self.browser_content_focus_handles
            .retain(|session_id, _| session_ids.contains(session_id));
        self.browser_content_bounds
            .retain(|session_id, _| session_ids.contains(session_id));
        for session_id in session_ids {
            self.browser_address_focus_handles
                .entry(session_id)
                .or_insert_with(|| cx.focus_handle().tab_stop(true));
            self.browser_content_focus_handles
                .entry(session_id)
                .or_insert_with(|| cx.focus_handle().tab_stop(true));
        }
    }

    fn default_terminal_cwd() -> PathBuf {
        std::env::current_dir().unwrap_or_else(|_| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/"))
        })
    }

    fn drain_control_requests(&mut self, cx: &mut Context<Self>) -> bool {
        let mut did_update = false;
        while let Ok(queued) = self.control_requests.try_recv() {
            let response = self.handle_control_request(queued.request, cx);
            if queued.respond_to.send(response).is_err() {
                tracing::warn!("control client disconnected before response");
            }
            did_update = true;
        }
        did_update
    }

    fn drain_terminal_events(&mut self) -> bool {
        let mut did_update = false;
        for terminal in &mut self.terminals {
            if terminal.drain_events() {
                did_update = true;
            }
        }
        did_update
    }

    #[allow(dead_code)]
    fn run_terminal_command_for_session(
        &mut self,
        session_id: SessionId,
        command: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        let command = command.into();
        match self.terminal_by_session_mut(session_id) {
            Some(terminal) => match terminal.run_command(command.clone()) {
                Ok(()) => {
                    self.push_event(
                        "info",
                        "session",
                        format!("queued UI command for session {session_id:?}"),
                    );
                }
                Err(message) => {
                    self.push_event(
                        "warn",
                        "session",
                        format!("failed to queue UI command for session {session_id:?}: {message}"),
                    );
                }
            },
            None => self.push_event(
                "warn",
                "session",
                format!("terminal session {session_id:?} not found for UI command"),
            ),
        }
        cx.notify();
    }

    fn write_terminal_input_from_ui_for_session(
        &mut self,
        session_id: SessionId,
        input: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        let input = input.into();
        match self.terminal_by_session_mut(session_id) {
            Some(terminal) => match terminal.write_input_from_ui(&input) {
                Ok(()) => {
                    self.push_event(
                        "info",
                        "session",
                        format!("wrote UI terminal input for session {session_id:?}"),
                    );
                }
                Err(message) => {
                    self.push_event(
                        "warn",
                        "session",
                        format!(
                            "failed to write UI terminal input for session {session_id:?}: {message}"
                        ),
                    );
                }
            },
            None => self.push_event(
                "warn",
                "session",
                format!("terminal session {session_id:?} not found for UI input"),
            ),
        }
        cx.notify();
    }

    fn replace_terminal_marked_text(
        &mut self,
        session_id: SessionId,
        text: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        let text = text.into();
        if text.is_empty() {
            self.terminal_marked_text.remove(&session_id);
        } else {
            self.terminal_marked_text.insert(session_id, text);
        }
        cx.notify();
    }

    fn clear_terminal_marked_text(&mut self, session_id: SessionId, cx: &mut Context<Self>) {
        if self.terminal_marked_text.remove(&session_id).is_some() {
            cx.notify();
        }
    }

    fn commit_terminal_ime_text(
        &mut self,
        session_id: SessionId,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        self.terminal_marked_text.remove(&session_id);
        if !text.is_empty() {
            self.write_terminal_input_from_ui_for_session(session_id, text, cx);
        } else {
            cx.notify();
        }
    }

    fn terminal_ime_bounds_for_session(
        &self,
        session_id: SessionId,
        element_bounds: Bounds<Pixels>,
    ) -> Bounds<Pixels> {
        let Some(terminal) = self.terminal_by_session(session_id) else {
            return element_bounds;
        };

        let snapshot = terminal.emulator.snapshot();
        let col = snapshot.cursor_col.min(snapshot.cols.saturating_sub(1)) as f32;
        let row = snapshot.cursor_row.min(snapshot.rows.saturating_sub(1)) as f32;
        let origin = Point::new(
            element_bounds.origin.x + px(TERMINAL_CELL_WIDTH_PX) * col,
            element_bounds.origin.y + px(TERMINAL_CELL_HEIGHT_PX) * row,
        );
        Bounds::new(
            origin,
            gpui::size(px(TERMINAL_CELL_WIDTH_PX), px(TERMINAL_CELL_HEIGHT_PX)),
        )
    }

    #[allow(dead_code)]
    fn interrupt_session_from_ui(&mut self, session_id: SessionId, cx: &mut Context<Self>) {
        match self.terminal_by_session_mut(session_id) {
            Some(terminal) => match terminal.interrupt_from_ui() {
                Ok(()) => {
                    self.push_event(
                        "info",
                        "session",
                        format!("interrupted session {session_id:?} from UI"),
                    );
                }
                Err(message) => {
                    self.push_event(
                        "warn",
                        "session",
                        format!("failed to interrupt session {session_id:?} from UI: {message}"),
                    );
                }
            },
            None => self.push_event(
                "warn",
                "session",
                format!("terminal session {session_id:?} not found for UI interrupt"),
            ),
        }
        cx.notify();
    }

    #[allow(dead_code)]
    fn restart_session_from_ui(&mut self, session_id: SessionId, cx: &mut Context<Self>) {
        let terminal_wake_tx = self.terminal_wake_tx.clone();
        match self.terminal_by_session_mut(session_id) {
            Some(terminal) => match terminal.restart(terminal_wake_tx) {
                Ok(()) => {
                    self.push_event(
                        "info",
                        "session",
                        format!("restarted session {session_id:?} from UI"),
                    );
                }
                Err(message) => {
                    self.push_event(
                        "warn",
                        "session",
                        format!("failed to restart session {session_id:?} from UI: {message}"),
                    );
                }
            },
            None => self.push_event(
                "warn",
                "session",
                format!("terminal session {session_id:?} not found for UI restart"),
            ),
        }
        cx.notify();
    }

    fn write_terminal_key_for_session(
        &mut self,
        session_id: SessionId,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.modifiers.platform && event.keystroke.key.eq_ignore_ascii_case("v") {
            if let Some(input) = cx
                .read_from_clipboard()
                .and_then(|item| item.text())
                .and_then(|text| paste_sequence_for_text(&text))
            {
                self.write_terminal_input_from_ui_for_session(session_id, input, cx);
            }
            cx.stop_propagation();
            return;
        }

        let mode = self
            .terminal_by_session(session_id)
            .map(|terminal| terminal.emulator.mode())
            .unwrap_or_default();
        let key = TerminalKey {
            key: event.keystroke.key.clone(),
            text: event.keystroke.key_char.clone(),
            modifiers: TerminalKeyModifiers {
                alt: event.keystroke.modifiers.alt,
                control: event.keystroke.modifiers.control,
                shift: event.keystroke.modifiers.shift,
                platform: event.keystroke.modifiers.platform,
            },
        };
        let Some(input) = input_sequence_for_key(&key, mode) else {
            return;
        };
        self.write_terminal_input_from_ui_for_session(session_id, input, cx);
    }

    fn sync_terminal_grid_metrics(
        &mut self,
        session_id: SessionId,
        metrics: TerminalGridMetrics,
        cx: &mut Context<Self>,
    ) {
        if let Some(terminal) = self.terminal_by_session_mut(session_id) {
            match terminal.sync_measured_size(metrics) {
                Ok(true) => {
                    self.push_event(
                        "info",
                        "session",
                        format!(
                            "resized measured terminal {session_id:?} to {}x{}",
                            metrics.cols, metrics.rows
                        ),
                    );
                    cx.notify();
                }
                Ok(false) => {}
                Err(message) => {
                    self.push_event(
                        "warn",
                        "session",
                        format!(
                            "failed to sync measured terminal size for {session_id:?}: {message}"
                        ),
                    );
                    cx.notify();
                }
            }
        }
    }

    fn terminal_by_session(&self, session_id: SessionId) -> Option<&TerminalRuntime> {
        self.terminals
            .iter()
            .find(|terminal| terminal.session_id() == session_id)
    }

    fn terminal_by_session_mut(&mut self, session_id: SessionId) -> Option<&mut TerminalRuntime> {
        self.terminals
            .iter_mut()
            .find(|terminal| terminal.session_id() == session_id)
    }

    fn workspace_by_id(&self, workspace_id: ah_core::WorkspaceId) -> Option<&Workspace> {
        self.workspaces
            .iter()
            .find(|workspace| workspace.id == workspace_id)
    }

    fn workspace_index_by_id(&self, workspace_id: ah_core::WorkspaceId) -> Option<usize> {
        self.workspaces
            .iter()
            .position(|workspace| workspace.id == workspace_id)
    }

    fn window_by_id(&self, window_id: WindowId) -> Option<&WorkspaceWindow> {
        self.workspaces
            .iter()
            .flat_map(|workspace| &workspace.windows)
            .find(|window| window.id == window_id)
    }

    fn window_by_id_mut(&mut self, window_id: WindowId) -> Option<&mut WorkspaceWindow> {
        self.workspaces
            .iter_mut()
            .flat_map(|workspace| &mut workspace.windows)
            .find(|window| window.id == window_id)
    }

    fn workspace_index_for_window(&self, window_id: WindowId) -> Option<usize> {
        self.workspaces.iter().position(|workspace| {
            workspace
                .windows
                .iter()
                .any(|window| window.id == window_id)
        })
    }

    fn activate_tab_for_session(
        &mut self,
        session_id: SessionId,
    ) -> Option<(WorkspaceId, WindowId, TabId)> {
        for workspace_index in 0..self.workspaces.len() {
            let workspace = &mut self.workspaces[workspace_index];
            for window in &mut workspace.windows {
                let Some(tab_id) = window.tabs.iter().find_map(|tab| match tab.content {
                    WindowContent::Terminal {
                        session_id: tab_session_id,
                    } if tab_session_id == session_id => Some(tab.id),
                    WindowContent::Web {
                        session_id: tab_session_id,
                        ..
                    } if tab_session_id == session_id => Some(tab.id),
                    WindowContent::Terminal { .. }
                    | WindowContent::Web { .. }
                    | WindowContent::FilePreview { .. } => None,
                }) else {
                    continue;
                };

                workspace.active_window_id = Some(window.id);
                window.activate_tab(tab_id);
                self.active_workspace_index = workspace_index;
                return Some((workspace.id, window.id, tab_id));
            }
        }

        None
    }

    #[allow(dead_code)]
    fn active_or_new_window_mut(
        workspace: &mut Workspace,
        default_title: impl Into<String>,
    ) -> &mut WorkspaceWindow {
        let window_id = workspace
            .active_window_id
            .filter(|window_id| {
                workspace
                    .windows
                    .iter()
                    .any(|window| window.id == *window_id)
            })
            .or_else(|| workspace.windows.first().map(|window| window.id));

        if let Some(window_id) = window_id {
            workspace.active_window_id = Some(window_id);
            return workspace
                .windows
                .iter_mut()
                .find(|window| window.id == window_id)
                .expect("active window id was selected from workspace windows");
        }

        workspace.push_window(WorkspaceWindow::new(default_title));
        workspace
            .windows
            .last_mut()
            .expect("push_window should insert a workspace window")
    }

    fn handle_control_request(
        &mut self,
        request: ControlRequest,
        cx: &mut Context<Self>,
    ) -> ControlResponse {
        let id = request.id;
        let result = match request.command {
            ControlCommand::Ping => ControlResult::Pong {
                protocol: "agenthouse-control/0.1".to_string(),
            },
            ControlCommand::Snapshot => ControlResult::Snapshot(self.control_snapshot()),
            ControlCommand::GetAppSettings => ControlResult::AppSettings {
                settings: self.app_settings_summary(),
            },
            ControlCommand::SetUiLanguage { language } => {
                let result = self.set_ui_language(language);
                self.refresh_settings_menu(cx);
                result
            }
            ControlCommand::SetUiThemeScheme { scheme } => {
                let result = self.set_ui_theme_scheme(scheme);
                self.refresh_settings_menu(cx);
                result
            }
            ControlCommand::ListEvents {
                since_sequence,
                limit,
            } => ControlResult::Events {
                events: self.list_events(since_sequence, limit),
            },
            ControlCommand::WatchEvents { .. } => control_error(
                "streaming_required",
                "watch_events must be used as a streaming control request",
            ),
            ControlCommand::CaptureSurface { window_id } => {
                ControlResult::SurfaceCapture(Box::new(self.capture_surface(window_id)))
            }
            ControlCommand::CaptureSessionSurface { session_id } => {
                match self.capture_session_surface(session_id) {
                    Some(surface) => ControlResult::SurfaceCapture(Box::new(surface)),
                    None => control_error("session_not_found", "session not found"),
                }
            }
            ControlCommand::CreateWorkspace { name, root } => self.create_workspace(name, root),
            ControlCommand::ListWorkspaces => ControlResult::Workspaces {
                workspaces: self.workspace_summaries(),
            },
            ControlCommand::ActivateWorkspace { workspace_id } => {
                match self.workspace_index_by_id(workspace_id) {
                    Some(index) => {
                        self.active_workspace_index = index;
                        let name = self.workspaces[index].name.clone();
                        self.push_event("info", "workspace", format!("activated workspace {name}"));
                        ControlResult::WorkspaceActivated { workspace_id }
                    }
                    None => control_error("workspace_not_found", "workspace not found"),
                }
            }
            ControlCommand::ListWindows { workspace_id } => {
                let Some(workspace) = workspace_id
                    .and_then(|workspace_id| self.workspace_by_id(workspace_id))
                    .or_else(|| self.workspaces.get(self.active_workspace_index))
                else {
                    return ControlResponse {
                        id,
                        result: ControlResult::Windows {
                            windows: Vec::new(),
                        },
                    };
                };
                ControlResult::Windows {
                    windows: window_summaries(workspace),
                }
            }
            ControlCommand::CreateWindow {
                workspace_id,
                title,
            } => self.create_window(workspace_id, title),
            ControlCommand::ListWindowTabs { window_id } => match self.window_by_id(window_id) {
                Some(window) => ControlResult::WindowTabs {
                    window_id,
                    tabs: window
                        .tabs
                        .iter()
                        .map(|tab| WindowTabSummary::from_tab(window, tab))
                        .collect(),
                },
                None => control_error("window_not_found", "window not found"),
            },
            ControlCommand::ListSessions { workspace_id } => ControlResult::Sessions {
                sessions: self.session_summaries_for_workspace(workspace_id),
            },
            ControlCommand::ActivateWindow {
                workspace_id,
                window_id,
            } => match self.workspace_index_by_id(workspace_id) {
                Some(index) => {
                    if self.workspaces[index]
                        .windows
                        .iter()
                        .any(|window| window.id == window_id)
                    {
                        self.active_workspace_index = index;
                        self.workspaces[index].active_window_id = Some(window_id);
                        self.push_event(
                            "info",
                            "window",
                            format!("activated window {window_id:?}"),
                        );
                        ControlResult::WindowActivated {
                            workspace_id,
                            window_id,
                        }
                    } else {
                        control_error("window_not_found", "window not found")
                    }
                }
                None => control_error("workspace_not_found", "workspace not found"),
            },
            ControlCommand::CloseWindow { window_id } => self.close_window(window_id),
            ControlCommand::ActivateWindowTab { window_id, tab_id } => {
                match self.workspace_index_for_window(window_id) {
                    Some(index) => {
                        self.active_workspace_index = index;
                        self.workspaces[index].active_window_id = Some(window_id);
                        let window = self.workspaces[index]
                            .windows
                            .iter_mut()
                            .find(|window| window.id == window_id)
                            .expect("window id was found in workspace");
                        if window.activate_tab(tab_id) {
                            self.push_event(
                                "info",
                                "window",
                                format!("activated tab {tab_id:?} in window {window_id:?}"),
                            );
                            ControlResult::WindowTabActivated { window_id, tab_id }
                        } else {
                            control_error("tab_not_found", "tab not found")
                        }
                    }
                    None => control_error("window_not_found", "window not found"),
                }
            }
            ControlCommand::CloseWindowTab { window_id, tab_id } => {
                self.close_window_tab(window_id, tab_id)
            }
            ControlCommand::MoveWindowTab {
                source_window_id,
                tab_id,
                target_window_id,
            } => self.move_window_tab(source_window_id, tab_id, target_window_id),
            ControlCommand::SetWorkspaceLayout { workspace_id, mode } => {
                self.set_workspace_layout(workspace_id, mode)
            }
            ControlCommand::OpenTerminalWindow {
                workspace_id,
                title,
                cwd,
            } => self.open_terminal_window(workspace_id, title, cwd),
            ControlCommand::OpenTerminalTab {
                window_id,
                title,
                cwd,
            } => self.open_terminal_tab(window_id, title, cwd, cx),
            ControlCommand::OpenWebWindow {
                workspace_id,
                title,
                url,
            } => self.open_web_window(workspace_id, title, url, cx),
            ControlCommand::OpenWebTab {
                window_id,
                title,
                url,
            } => self.open_web_tab(window_id, title, url, cx),
            ControlCommand::SplitWindow {
                window_id,
                direction,
            } => self.split_window(window_id, split_direction_from_control(direction), cx),
            ControlCommand::ListBrowserSessions { workspace_id } => {
                ControlResult::BrowserSessions {
                    sessions: self.browser_session_summaries_for_workspace(workspace_id),
                }
            }
            ControlCommand::GetBrowserSession { session_id } => {
                self.get_browser_session(session_id)
            }
            ControlCommand::CaptureBrowserSurface { session_id } => {
                self.capture_browser_surface(session_id)
            }
            ControlCommand::BrowserNavigate { session_id, url } => {
                self.navigate_browser(session_id, url, cx)
            }
            ControlCommand::BrowserAction { session_id, action } => {
                self.apply_browser_action(session_id, action, cx)
            }
            ControlCommand::SendBrowserInput { session_id, input } => {
                self.apply_browser_input(session_id, input)
            }
            ControlCommand::ResizeBrowser {
                session_id,
                viewport,
            } => self.resize_browser(session_id, viewport),
            ControlCommand::RunTerminalCommand {
                session_id,
                command,
            } => {
                let activated_tab = self.activate_tab_for_session(session_id);
                match self.terminal_by_session_mut(session_id) {
                    Some(terminal) => match terminal.run_command(command) {
                        Ok(()) => {
                            if let Some((workspace_id, window_id, tab_id)) = activated_tab {
                                self.push_event(
                                    "info",
                                    "window",
                                    format!(
                                        "activated session tab {tab_id:?} in window {window_id:?} workspace {workspace_id:?}"
                                    ),
                                );
                            }
                            self.push_event(
                                "info",
                                "session",
                                format!("queued command for session {session_id:?}"),
                            );
                            ControlResult::CommandQueued { session_id }
                        }
                        Err(message) => control_error("command_failed", message),
                    },
                    None => control_error("session_not_found", "session not found"),
                }
            }
            ControlCommand::WriteTerminalInput { session_id, input } => {
                let activated_tab = self.activate_tab_for_session(session_id);
                match self.terminal_by_session_mut(session_id) {
                    Some(terminal) => match terminal.write_input(&input) {
                        Ok(()) => {
                            if let Some((workspace_id, window_id, tab_id)) = activated_tab {
                                self.push_event(
                                    "info",
                                    "window",
                                    format!(
                                        "activated input target tab {tab_id:?} in window {window_id:?} workspace {workspace_id:?}"
                                    ),
                                );
                            }
                            self.push_event(
                                "info",
                                "session",
                                format!("wrote terminal input for session {session_id:?}"),
                            );
                            ControlResult::TerminalInputWritten { session_id }
                        }
                        Err(message) => control_error("terminal_input_failed", message),
                    },
                    None => control_error("session_not_found", "session not found"),
                }
            }
            ControlCommand::SendTerminalKey { session_id, key } => {
                let activated_tab = self.activate_tab_for_session(session_id);
                match self.terminal_by_session_mut(session_id) {
                    Some(terminal) => {
                        let mode = terminal.emulator.mode();
                        let key_name = key.key.clone();
                        let key = terminal_key_from_control(key);
                        match input_sequence_for_key(&key, mode) {
                            Some(input) => match terminal.write_input(&input) {
                                Ok(()) => {
                                    if let Some((workspace_id, window_id, tab_id)) = activated_tab {
                                        self.push_event(
                                            "info",
                                            "window",
                                            format!(
                                                "activated key target tab {tab_id:?} in window {window_id:?} workspace {workspace_id:?}"
                                            ),
                                        );
                                    }
                                    self.push_event(
                                        "info",
                                        "session",
                                        format!(
                                            "sent terminal key {key_name} for session {session_id:?}"
                                        ),
                                    );
                                    ControlResult::TerminalKeySent { session_id }
                                }
                                Err(message) => control_error("terminal_key_failed", message),
                            },
                            None => control_error(
                                "terminal_key_unsupported",
                                format!("terminal key {key_name} is unsupported"),
                            ),
                        }
                    }
                    None => control_error("session_not_found", "session not found"),
                }
            }
            ControlCommand::InterruptSession { session_id } => {
                let activated_tab = self.activate_tab_for_session(session_id);
                let result = match self.terminal_by_session_mut(session_id) {
                    Some(terminal) => match terminal.interrupt() {
                        Ok(()) => ControlResult::SessionInterrupted {
                            session: session_summary(terminal),
                        },
                        Err(message) => control_error("interrupt_failed", message),
                    },
                    None => control_error("session_not_found", "session not found"),
                };
                if matches!(result, ControlResult::SessionInterrupted { .. }) {
                    if let Some((workspace_id, window_id, tab_id)) = activated_tab {
                        self.push_event(
                            "info",
                            "window",
                            format!(
                                "activated interrupt target tab {tab_id:?} in window {window_id:?} workspace {workspace_id:?}"
                            ),
                        );
                    }
                    self.push_event(
                        "info",
                        "session",
                        format!("interrupted session {session_id:?}"),
                    );
                }
                result
            }
            ControlCommand::TerminateSession { session_id } => {
                let activated_tab = self.activate_tab_for_session(session_id);
                let result = match self.terminal_by_session_mut(session_id) {
                    Some(terminal) => match terminal.terminate() {
                        Ok(()) => ControlResult::SessionTerminated {
                            session: session_summary(terminal),
                        },
                        Err(message) => control_error("terminate_failed", message),
                    },
                    None => control_error("session_not_found", "session not found"),
                };
                if matches!(result, ControlResult::SessionTerminated { .. }) {
                    if let Some((workspace_id, window_id, tab_id)) = activated_tab {
                        self.push_event(
                            "info",
                            "window",
                            format!(
                                "activated terminate target tab {tab_id:?} in window {window_id:?} workspace {workspace_id:?}"
                            ),
                        );
                    }
                    self.push_event(
                        "info",
                        "session",
                        format!("terminated session {session_id:?}"),
                    );
                }
                result
            }
            ControlCommand::RestartSession { session_id } => {
                let activated_tab = self.activate_tab_for_session(session_id);
                let terminal_wake_tx = self.terminal_wake_tx.clone();
                let result = match self.terminal_by_session_mut(session_id) {
                    Some(terminal) => match terminal.restart(terminal_wake_tx) {
                        Ok(()) => ControlResult::SessionRestarted {
                            session: session_summary(terminal),
                        },
                        Err(message) => control_error("restart_failed", message),
                    },
                    None => control_error("session_not_found", "session not found"),
                };
                if matches!(result, ControlResult::SessionRestarted { .. }) {
                    if let Some((workspace_id, window_id, tab_id)) = activated_tab {
                        self.push_event(
                            "info",
                            "window",
                            format!(
                                "activated restart target tab {tab_id:?} in window {window_id:?} workspace {workspace_id:?}"
                            ),
                        );
                    }
                    self.push_event(
                        "info",
                        "session",
                        format!("restarted session {session_id:?}"),
                    );
                }
                result
            }
            ControlCommand::ResizeTerminal {
                session_id,
                cols,
                rows,
            } => {
                let activated_tab = self.activate_tab_for_session(session_id);
                match self.terminal_by_session_mut(session_id) {
                    Some(terminal) => match terminal.resize(cols, rows) {
                        Ok(()) => {
                            if let Some((workspace_id, window_id, tab_id)) = activated_tab {
                                self.push_event(
                                    "info",
                                    "window",
                                    format!(
                                        "activated resize target tab {tab_id:?} in window {window_id:?} workspace {workspace_id:?}"
                                    ),
                                );
                            }
                            self.push_event(
                                "info",
                                "session",
                                format!("resized terminal for session {session_id:?}"),
                            );
                            ControlResult::TerminalResized {
                                session_id,
                                cols,
                                rows,
                            }
                        }
                        Err(message) => control_error("resize_failed", message),
                    },
                    None => control_error("session_not_found", "session not found"),
                }
            }
            ControlCommand::GetSession { session_id } => match self.terminal_by_session(session_id)
            {
                Some(terminal) => ControlResult::Session {
                    session: session_summary(terminal),
                },
                None => control_error("session_not_found", "session not found"),
            },
            ControlCommand::AckSessionRing { session_id } => {
                match self.terminal_by_session_mut(session_id) {
                    Some(terminal) => {
                        terminal.view.ring.acknowledge();
                        let session = session_summary(terminal);
                        self.push_event(
                            "info",
                            "session",
                            format!("acknowledged notification ring for session {session_id:?}"),
                        );
                        ControlResult::SessionRingAcknowledged { session }
                    }
                    None => control_error("session_not_found", "session not found"),
                }
            }
            ControlCommand::ListBlocks { session_id } => match self.terminal_by_session(session_id)
            {
                Some(terminal) => ControlResult::Blocks {
                    session_id,
                    blocks: terminal
                        .view
                        .blocks
                        .iter()
                        .map(|row| BlockSummary::from_block(&row.block))
                        .collect(),
                },
                None => control_error("session_not_found", "session not found"),
            },
            ControlCommand::ForwardBlock {
                source_session_id,
                block_id,
                target_session_id,
            } => self.forward_block(source_session_id, block_id, target_session_id),
        };
        self.persist_state();
        cx.notify();
        ControlResponse { id, result }
    }

    fn push_event(
        &mut self,
        level: impl Into<String>,
        topic: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.events.push(ControlEvent {
            sequence: self.next_event_sequence,
            level: level.into(),
            topic: topic.into(),
            message: message.into(),
        });
        self.next_event_sequence += 1;
        const MAX_EVENTS: usize = 1_000;
        if self.events.len() > MAX_EVENTS {
            let drop_count = self.events.len() - MAX_EVENTS;
            self.events.drain(..drop_count);
        }
    }

    fn list_events(&self, since_sequence: Option<u64>, limit: Option<usize>) -> Vec<ControlEvent> {
        let limit = limit.unwrap_or(100).min(500);
        let mut events: Vec<_> = self
            .events
            .iter()
            .filter(|event| since_sequence.is_none_or(|sequence| event.sequence > sequence))
            .rev()
            .take(limit)
            .cloned()
            .collect();
        events.reverse();
        events
    }

    fn refresh_browser_session(&mut self, session_id: SessionId) {
        let Some(browser) = self.browser_by_session_mut(session_id) else {
            self.push_event(
                "warn",
                "web",
                format!("browser session {session_id:?} disappeared before refresh"),
            );
            return;
        };
        let url = browser.state.current_url.clone();

        match browser.queue(BrowserWorkerCommand::Snapshot) {
            Ok(()) => self.push_event(
                "info",
                "web",
                format!("queued browser session {session_id:?} refresh for {url}"),
            ),
            Err(error) => self.push_event(
                "warn",
                "web",
                format!(
                    "failed to queue browser session {session_id:?} refresh for {url}: {error}"
                ),
            ),
        }
    }

    fn drain_browser_events(&mut self) -> bool {
        let mut did_update = false;
        let mut messages = Vec::new();
        let mut url_updates = Vec::new();
        for browser in &mut self.browsers {
            let session_id = browser.session_id();
            let url_before = browser.state.current_url.clone();
            if browser.drain_events() {
                did_update = true;
                let status =
                    browser
                        .pending_status
                        .take()
                        .unwrap_or_else(|| match browser.state.status {
                            BrowserLoadStatus::Loading => "browser loading".to_string(),
                            BrowserLoadStatus::Idle => "browser idle".to_string(),
                            BrowserLoadStatus::Loaded => "browser ready".to_string(),
                            BrowserLoadStatus::Failed => "browser failed".to_string(),
                        });
                let url = browser.state.current_url.clone();
                messages.push((session_id, url.clone(), status));
                if url != url_before {
                    url_updates.push((session_id, url));
                }
            }
        }
        for (session_id, url) in url_updates {
            self.sync_web_tab_url(session_id, url);
        }
        for (session_id, url, status) in messages {
            self.push_event(
                "info",
                "web",
                format!("browser session {session_id:?} refreshed for {url}: {status}"),
            );
        }
        did_update
    }

    fn sync_web_tab_url(&mut self, session_id: SessionId, url: String) {
        for workspace in &mut self.workspaces {
            for window in &mut workspace.windows {
                for tab in &mut window.tabs {
                    if let WindowContent::Web {
                        session_id: tab_session_id,
                        url: tab_url,
                    } = &mut tab.content
                        && *tab_session_id == session_id
                    {
                        *tab_url = url.clone();
                    }
                }
            }
        }
    }

    fn navigate_browser_from_ui(
        &mut self,
        session_id: SessionId,
        address: String,
        cx: &mut Context<Self>,
    ) {
        let address = address.trim();
        if address.is_empty() {
            return;
        }
        let url = normalize_browser_address(address);
        match self.browser_by_session_mut(session_id) {
            Some(browser) => {
                if let Err(error) = browser.navigate(url.clone()) {
                    self.browser_address_inputs
                        .insert(session_id, address.to_string());
                    self.push_event(
                        "warn",
                        "web",
                        format!(
                            "failed to navigate browser session {session_id:?} to {url}: {error}"
                        ),
                    );
                } else {
                    self.clear_browser_address_edit(session_id);
                    self.sync_web_tab_url(session_id, url.clone());
                    self.push_event(
                        "info",
                        "web",
                        format!("navigated browser session {session_id:?} to {url}"),
                    );
                    self.persist_state();
                }
            }
            None => {
                self.push_event(
                    "warn",
                    "web",
                    format!("browser session {session_id:?} not found for navigation"),
                );
            }
        }
        cx.notify();
    }

    fn apply_browser_action_from_ui(
        &mut self,
        session_id: SessionId,
        action: BrowserAction,
        cx: &mut Context<Self>,
    ) {
        let action_label = format!("{action:?}");
        match self.browser_by_session_mut(session_id) {
            Some(browser) => {
                if let Err(error) = browser.apply_action(&action) {
                    self.push_event(
                        "warn",
                        "web",
                        format!(
                            "browser action {action_label} failed for session {session_id:?}: {error}"
                        ),
                    );
                } else {
                    self.push_event(
                        "info",
                        "web",
                        format!("browser action {action_label} queued for session {session_id:?}"),
                    );
                    self.persist_state();
                }
            }
            None => self.push_event(
                "warn",
                "web",
                format!("browser session {session_id:?} not found for action"),
            ),
        }
        cx.notify();
    }

    fn send_browser_input_from_ui(
        &mut self,
        session_id: SessionId,
        input: BrowserInput,
        cx: &mut Context<Self>,
    ) {
        match self.browser_by_session_mut(session_id) {
            Some(browser) => {
                let current_url = browser.state.current_url.clone();
                match browser.input(input) {
                    Ok(status) => {
                        self.push_event(
                            "info",
                            "web",
                            format!(
                                "browser session {session_id:?} handled input for {current_url}: {status}"
                            ),
                        );
                    }
                    Err(error) => {
                        self.push_event(
                            "warn",
                            "web",
                            format!(
                                "browser session {session_id:?} failed input for {current_url}: {error}"
                            ),
                        );
                    }
                }
            }
            None => {
                self.push_event(
                    "warn",
                    "web",
                    format!("browser session {session_id:?} not found for input"),
                );
            }
        }
        cx.notify();
    }

    fn browser_local_point(
        &self,
        session_id: SessionId,
        window_position: Point<Pixels>,
    ) -> Option<(i32, i32)> {
        let bounds = self.browser_content_bounds.get(&session_id)?;
        if !bounds.contains(&window_position) {
            return None;
        }
        let local = window_position - bounds.origin;
        Some((
            local.x.as_f32().round().max(0.0) as i32,
            local.y.as_f32().round().max(0.0) as i32,
        ))
    }

    fn web_preview_for_session(&self, session_id: SessionId, url: &str) -> WebPreviewSnapshot {
        self.browser_by_session(session_id)
            .map(BrowserRuntime::preview_snapshot)
            .unwrap_or_else(|| WebPreviewSnapshot::pending(url))
    }

    fn persisted_control_state(&self) -> PersistedControlState {
        PersistedControlState {
            active_workspace_id: self
                .workspaces
                .get(self.active_workspace_index)
                .map(|w| w.id),
            ui_language: Some(self.ui_language.preference()),
            ui_theme_scheme: Some(self.ui_theme_scheme.control_code().to_string()),
            ui_theme_mode: Some(FIXED_UI_THEME_MODE.to_string()),
            next_event_sequence: self.next_event_sequence,
            events: self.events.clone(),
            session_rings: self
                .terminals
                .iter()
                .map(|terminal| PersistedSessionRing {
                    session_id: terminal.session_id(),
                    state: ring_state_label(&terminal.view.ring.state).to_string(),
                    summary: terminal.view.ring.summary.to_string(),
                    unread_count: terminal.view.ring.unread_count,
                })
                .collect(),
        }
    }

    fn control_snapshot(&self) -> ControlSnapshot {
        ControlSnapshot {
            active_workspace_id: self
                .workspaces
                .get(self.active_workspace_index)
                .map(|w| w.id),
            workspaces: self.workspace_summaries(),
            windows: self.workspaces.iter().flat_map(window_summaries).collect(),
            sessions: self.terminals.iter().map(session_summary).collect(),
        }
    }

    fn workspace_summaries(&self) -> Vec<WorkspaceSummary> {
        self.workspaces
            .iter()
            .enumerate()
            .map(|(index, workspace)| {
                WorkspaceSummary::from_workspace(workspace, index == self.active_workspace_index)
            })
            .collect()
    }

    fn session_summaries_for_workspace(
        &self,
        workspace_id: Option<WorkspaceId>,
    ) -> Vec<SessionSummary> {
        let Some(workspace) = workspace_id
            .and_then(|workspace_id| self.workspace_by_id(workspace_id))
            .or_else(|| self.workspaces.get(self.active_workspace_index))
        else {
            return Vec::new();
        };

        workspace
            .windows
            .iter()
            .flat_map(|window| &window.tabs)
            .filter_map(|tab| match &tab.content {
                WindowContent::Terminal { session_id } => {
                    self.terminal_by_session(*session_id).map(session_summary)
                }
                WindowContent::Web { .. } | WindowContent::FilePreview { .. } => None,
            })
            .collect()
    }

    fn browser_session_summaries_for_workspace(
        &self,
        workspace_id: Option<WorkspaceId>,
    ) -> Vec<BrowserSessionSummary> {
        let referenced = self.browser_session_ids_for_workspace(workspace_id);
        self.browsers
            .iter()
            .filter(|browser| referenced.contains(&browser.session_id()))
            .map(|browser| BrowserSessionSummary::from_state(&browser.state))
            .collect()
    }

    fn get_browser_session(&self, session_id: SessionId) -> ControlResult {
        match self.browser_by_session(session_id) {
            Some(browser) => ControlResult::BrowserSession {
                session: BrowserSessionSummary::from_state(&browser.state),
            },
            None => control_error("browser_session_not_found", "browser session not found"),
        }
    }

    fn capture_browser_surface(&self, session_id: SessionId) -> ControlResult {
        match self.browser_by_session(session_id) {
            Some(browser) => ControlResult::BrowserSurface {
                surface: browser.surface_snapshot(),
            },
            None => control_error("browser_session_not_found", "browser session not found"),
        }
    }

    fn navigate_browser(
        &mut self,
        session_id: SessionId,
        url: String,
        _cx: &mut Context<Self>,
    ) -> ControlResult {
        let activated_tab = self.activate_tab_for_session(session_id);
        let result = match self.browser_by_session_mut(session_id) {
            Some(browser) => {
                if let Err(error) = browser.navigate(url.clone()) {
                    self.push_event(
                        "warn",
                        "web",
                        format!(
                            "failed to navigate browser session {session_id:?} to {url}: {error}"
                        ),
                    );
                    return control_error("browser_navigation_failed", error);
                }
                let summary = BrowserSessionSummary::from_state(&browser.state);
                self.clear_browser_address_edit(session_id);
                self.sync_web_tab_url(session_id, url.clone());
                ControlResult::BrowserSession { session: summary }
            }
            None => return control_error("browser_session_not_found", "browser session not found"),
        };

        if let Some((workspace_id, window_id, tab_id)) = activated_tab {
            self.push_event(
                "info",
                "window",
                format!(
                    "activated browser tab {tab_id:?} in window {window_id:?} workspace {workspace_id:?}"
                ),
            );
        }
        self.push_event(
            "info",
            "web",
            format!("navigated browser session {session_id:?} to {url}"),
        );
        result
    }

    fn apply_browser_action(
        &mut self,
        session_id: SessionId,
        action: BrowserAction,
        cx: &mut Context<Self>,
    ) -> ControlResult {
        match action {
            BrowserAction::Navigate { url } => self.navigate_browser(session_id, url, cx),
            BrowserAction::Snapshot => self.capture_browser_surface(session_id),
            action => {
                let Some(browser) = self.browser_by_session_mut(session_id) else {
                    return control_error("browser_session_not_found", "browser session not found");
                };
                let action_label = format!("{action:?}");
                let value = match browser.apply_action(&action) {
                    Ok(value) => value,
                    Err(error) => {
                        let code = if action.requires_control_backend() {
                            "browser_control_backend_required"
                        } else {
                            "browser_action_failed"
                        };
                        self.push_event(
                            "warn",
                            "web",
                            format!(
                                "browser action {action_label} failed for session {session_id:?}: {error}"
                            ),
                        );
                        return control_error(code, error);
                    }
                };
                ControlResult::BrowserActionApplied {
                    result: BrowserActionResult {
                        session: browser.state.clone(),
                        message: format!("browser action applied: {action_label}"),
                        value,
                    },
                }
            }
        }
    }

    fn apply_browser_input(&mut self, session_id: SessionId, input: BrowserInput) -> ControlResult {
        let Some(browser) = self.browser_by_session_mut(session_id) else {
            return control_error("browser_session_not_found", "browser session not found");
        };
        let input_label = format!("{input:?}");
        match browser.input(input) {
            Ok(status) => {
                let session = browser.state.clone();
                self.push_event(
                    "info",
                    "web",
                    format!("browser session {session_id:?} queued input: {input_label}"),
                );
                ControlResult::BrowserActionApplied {
                    result: BrowserActionResult {
                        session,
                        message: format!("browser input queued: {status}"),
                        value: None,
                    },
                }
            }
            Err(error) => {
                self.push_event(
                    "warn",
                    "web",
                    format!(
                        "browser session {session_id:?} failed to queue input {input_label}: {error}"
                    ),
                );
                control_error("browser_input_failed", error)
            }
        }
    }

    fn resize_browser(&mut self, session_id: SessionId, viewport: ViewportSize) -> ControlResult {
        match self.browser_by_session_mut(session_id) {
            Some(browser) => {
                if let Err(error) = browser.resize(viewport) {
                    self.push_event(
                        "warn",
                        "web",
                        format!("failed to resize browser session {session_id:?}: {error}"),
                    );
                    return control_error("browser_resize_failed", error);
                }
                self.push_event(
                    "info",
                    "web",
                    format!(
                        "resized browser session {session_id:?} to {}x{}",
                        viewport.width, viewport.height
                    ),
                );
                ControlResult::BrowserResized {
                    session_id,
                    viewport,
                }
            }
            None => control_error("browser_session_not_found", "browser session not found"),
        }
    }

    fn capture_surface(&self, window_id: Option<WindowId>) -> SurfaceCapture {
        let Some(active_workspace) = self.workspaces.get(self.active_workspace_index) else {
            let snapshot_path = write_structured_surface_snapshot(json!({
                "active_workspace_id": null,
                "active_window_id": null,
                "target_window_id": window_id,
                "workspaces": [],
                "windows": [],
            }));
            return SurfaceCapture {
                mode: "structured_surface".to_string(),
                active_workspace_id: None,
                active_window_id: None,
                target_window_id: window_id,
                workspace_name: None,
                window_title: None,
                content_type: None,
                target_url: None,
                target_path: None,
                session: None,
                recent_blocks: Vec::new(),
                terminal_tail: None,
                screenshot_path: None,
                snapshot_path,
                note: "no workspace is open".to_string(),
            };
        };
        let active_window_id = active_workspace.active_window_id;
        let target_window_id = window_id.or(active_window_id);
        let target = target_window_id
            .and_then(|target_window_id| {
                self.workspaces.iter().find_map(|workspace| {
                    workspace
                        .windows
                        .iter()
                        .find(|window| window.id == target_window_id)
                        .map(|window| (workspace, window))
                })
            })
            .or_else(|| {
                active_workspace
                    .windows
                    .first()
                    .map(|window| (active_workspace, window))
            });
        let target_workspace = target.map(|(workspace, _)| workspace);
        let target_window = target.map(|(_, window)| window);

        let active_tab = target_window.and_then(WorkspaceWindow::active_tab);
        let (window_title, content_type, target_url, target_path, session_id, browser_session_id) =
            match target_window {
                Some(window) => match active_tab.map(|tab| &tab.content) {
                    Some(WindowContent::Terminal { session_id }) => (
                        Some(window.title.clone()),
                        Some("terminal".to_string()),
                        None,
                        None,
                        Some(*session_id),
                        None,
                    ),
                    Some(WindowContent::Web { session_id, url }) => (
                        Some(window.title.clone()),
                        Some("web".to_string()),
                        Some(url.clone()),
                        None,
                        None,
                        Some(*session_id),
                    ),
                    Some(WindowContent::FilePreview { path }) => (
                        Some(window.title.clone()),
                        Some("file_preview".to_string()),
                        None,
                        Some(path.clone()),
                        None,
                        None,
                    ),
                    None => (
                        Some(window.title.clone()),
                        Some("empty".to_string()),
                        None,
                        None,
                        None,
                        None,
                    ),
                },
                None => (None, None, None, None, None, None),
            };

        let terminal = session_id.and_then(|session_id| self.terminal_by_session(session_id));
        let recent_blocks: Vec<BlockSummary> = terminal
            .map(|terminal| {
                terminal
                    .view
                    .blocks
                    .iter()
                    .take(8)
                    .map(|row| BlockSummary::from_block(&row.block))
                    .collect()
            })
            .unwrap_or_default();
        let session_summary = terminal.map(session_summary);
        let terminal_tail = terminal.map(|terminal| {
            wrap_long_lines(
                &tail_chars(&terminal.view.transcript, MAX_BLOCK_DISPLAY_CHARS),
                MAX_DISPLAY_LINE_CHARS,
            )
        });
        let terminal_snapshot = terminal.map(|terminal| terminal.emulator.snapshot());
        let terminal_screen = terminal_snapshot.as_ref().map(|snapshot| snapshot.text());
        let file_preview = target_path
            .as_ref()
            .map(|path| file_preview_snapshot(path.as_path()));
        let browser_surface = browser_session_id
            .and_then(|session_id| self.browser_by_session(session_id))
            .map(BrowserRuntime::surface_snapshot);
        let browser_session = browser_surface
            .as_ref()
            .map(|surface| BrowserSessionSummary::from_state(&surface.session));
        let web_preview = browser_session_id
            .and_then(|session_id| target_url.as_ref().map(|url| (session_id, url)))
            .map(|(session_id, url)| self.web_preview_for_session(session_id, url));
        let snapshot_path = write_structured_surface_snapshot(json!({
            "active_workspace_id": active_workspace.id,
            "active_window_id": active_window_id,
            "target_window_id": target_window.map(|window| window.id),
            "workspace_name": target_workspace.map(|workspace| workspace.name.clone()),
            "window_title": window_title,
            "content_type": content_type,
            "target_url": target_url,
            "target_path": target_path,
            "session": session_summary,
            "recent_blocks": recent_blocks,
            "terminal_tail": terminal_tail,
            "terminal_screen": terminal_screen,
            "terminal_snapshot": terminal_snapshot,
            "file_preview": file_preview,
            "browser_session": browser_session,
            "browser_surface": browser_surface,
            "web_preview": web_preview,
        }));
        let screenshot = capture_screen_png();
        let (mode, screenshot_path, note) = match (&screenshot, &snapshot_path) {
            (Some(path), _) => (
                "png_and_structured_surface".to_string(),
                Some(path.clone()),
                "full-screen PNG capture and structured visual state are available".to_string(),
            ),
            (None, Some(_)) => (
                "structured_snapshot".to_string(),
                None,
                "structured visual state was written to snapshot_path; PNG capture was not produced by screencapture"
                    .to_string(),
            ),
            (None, None) => (
                "structured_surface".to_string(),
                None,
                "structured visual state is available; PNG capture was not produced by screencapture"
                    .to_string(),
            ),
        };

        SurfaceCapture {
            mode,
            active_workspace_id: Some(active_workspace.id),
            active_window_id,
            target_window_id: target_window.map(|window| window.id),
            workspace_name: target_workspace.map(|workspace| workspace.name.clone()),
            window_title,
            content_type,
            target_url,
            target_path,
            session: session_summary,
            recent_blocks,
            terminal_tail,
            screenshot_path,
            snapshot_path,
            note,
        }
    }

    fn capture_session_surface(&self, session_id: SessionId) -> Option<SurfaceCapture> {
        let active_workspace = self.workspaces.get(self.active_workspace_index)?;
        let active_window_id = active_workspace.active_window_id;
        let (target_workspace, target_window) = self.workspaces.iter().find_map(|workspace| {
            workspace.windows.iter().find_map(|window| {
                let has_session = window.tabs.iter().any(|tab| {
                    matches!(
                        tab.content,
                        WindowContent::Terminal {
                            session_id: tab_session_id
                        } if tab_session_id == session_id
                    )
                });
                has_session.then_some((workspace, window))
            })
        })?;
        let terminal = self.terminal_by_session(session_id)?;
        let recent_blocks: Vec<BlockSummary> = terminal
            .view
            .blocks
            .iter()
            .take(8)
            .map(|row| BlockSummary::from_block(&row.block))
            .collect();
        let session_summary = Some(session_summary(terminal));
        let terminal_tail = Some(wrap_long_lines(
            &tail_chars(&terminal.view.transcript, MAX_BLOCK_DISPLAY_CHARS),
            MAX_DISPLAY_LINE_CHARS,
        ));
        let terminal_snapshot = Some(terminal.emulator.snapshot());
        let terminal_screen = terminal_snapshot.as_ref().map(|snapshot| snapshot.text());
        let snapshot_path = write_structured_surface_snapshot(json!({
            "active_workspace_id": active_workspace.id,
            "active_window_id": active_window_id,
            "target_window_id": target_window.id,
            "workspace_name": target_workspace.name.clone(),
            "window_title": target_window.title.clone(),
            "content_type": "terminal",
            "target_url": null,
            "target_path": null,
            "session": session_summary,
            "recent_blocks": recent_blocks,
            "terminal_tail": terminal_tail,
            "terminal_screen": terminal_screen,
            "terminal_snapshot": terminal_snapshot,
        }));
        let screenshot = capture_screen_png();
        let (mode, screenshot_path, note) = match (&screenshot, &snapshot_path) {
            (Some(path), _) => (
                "png_and_structured_surface".to_string(),
                Some(path.clone()),
                "full-screen PNG capture and structured visual state are available".to_string(),
            ),
            (None, Some(_)) => (
                "structured_snapshot".to_string(),
                None,
                "structured visual state was written to snapshot_path; PNG capture was not produced by screencapture"
                    .to_string(),
            ),
            (None, None) => (
                "structured_surface".to_string(),
                None,
                "structured visual state is available; PNG capture was not produced by screencapture"
                    .to_string(),
            ),
        };

        Some(SurfaceCapture {
            mode,
            active_workspace_id: Some(active_workspace.id),
            active_window_id,
            target_window_id: Some(target_window.id),
            workspace_name: Some(target_workspace.name.clone()),
            window_title: Some(target_window.title.clone()),
            content_type: Some("terminal".to_string()),
            target_url: None,
            target_path: None,
            session: session_summary,
            recent_blocks,
            terminal_tail,
            screenshot_path,
            snapshot_path,
            note,
        })
    }

    fn create_workspace(&mut self, name: String, root: Option<PathBuf>) -> ControlResult {
        let root = root.unwrap_or_else(Self::default_terminal_cwd);
        let root = canonical_workspace_root(root);
        if let Some(index) = self
            .workspaces
            .iter()
            .position(|workspace| workspace.root.as_ref() == Some(&root))
        {
            self.active_workspace_index = index;
            self.ensure_workspaces_have_terminal_panes();
            let summary = WorkspaceSummary::from_workspace(self.active_workspace(), true);
            self.push_event(
                "info",
                "workspace",
                format!("activated existing workspace {:?}", summary.id),
            );
            return ControlResult::WorkspaceCreated { workspace: summary };
        }

        let mut workspace = workspace_for_root(self.workspaces.len(), root.clone());
        if !name.trim().is_empty() {
            workspace.name = name;
        }
        let workspace_id = workspace.id;
        self.workspaces.push(workspace);
        self.active_workspace_index = self.workspaces.len() - 1;
        let _ = self.add_terminal_pane_to_workspace(self.active_workspace_index);
        let summary = WorkspaceSummary::from_workspace(self.active_workspace(), true);
        self.push_event(
            "info",
            "workspace",
            format!("created workspace {workspace_id:?} at {}", root.display()),
        );
        ControlResult::WorkspaceCreated { workspace: summary }
    }

    fn create_window(&mut self, workspace_id: WorkspaceId, title: String) -> ControlResult {
        let Some(index) = self.workspace_index_by_id(workspace_id) else {
            return control_error("workspace_not_found", "workspace not found");
        };

        let window = WorkspaceWindow::new(title);
        let window_id = window.id;
        self.workspaces[index].push_window(window);
        self.active_workspace_index = index;
        let workspace = &self.workspaces[index];
        let window = workspace
            .windows
            .iter()
            .find(|window| window.id == window_id)
            .expect("created window should exist in workspace");
        let summary = WindowSummary::from_window(workspace, window);
        self.push_event(
            "info",
            "window",
            format!("created window {window_id:?} in workspace {workspace_id:?}"),
        );
        ControlResult::WindowCreated {
            workspace_id,
            window: summary,
        }
    }

    fn close_window(&mut self, window_id: WindowId) -> ControlResult {
        let Some(index) = self.workspace_index_for_window(window_id) else {
            return control_error("window_not_found", "window not found");
        };
        let workspace_id = self.workspaces[index].id;
        let (session_ids, browser_session_ids) = {
            let workspace = &mut self.workspaces[index];
            let Some(position) = workspace
                .windows
                .iter()
                .position(|window| window.id == window_id)
            else {
                return control_error("window_not_found", "window not found");
            };

            let window = workspace.windows.remove(position);
            let session_ids = terminal_session_ids_for_window(&window);
            let browser_session_ids = browser_session_ids_for_window(&window);
            workspace.active_window_id = workspace
                .active_window_id
                .filter(|active_window_id| *active_window_id != window_id)
                .or_else(|| workspace.windows.first().map(|window| window.id));
            (session_ids, browser_session_ids)
        };
        for session_id in session_ids {
            self.remove_terminal_runtime(session_id);
        }
        for session_id in browser_session_ids {
            self.remove_browser_runtime(session_id);
        }
        self.active_workspace_index = index;
        self.push_event(
            "info",
            "window",
            format!("closed window {window_id:?} in workspace {workspace_id:?}"),
        );
        ControlResult::WindowClosed {
            workspace_id,
            window_id,
        }
    }

    fn close_window_tab(&mut self, window_id: WindowId, tab_id: TabId) -> ControlResult {
        let Some(index) = self.workspace_index_for_window(window_id) else {
            return control_error("window_not_found", "window not found");
        };
        let (session_id, browser_session_id) = {
            let Some(window) = self.workspaces[index]
                .windows
                .iter_mut()
                .find(|window| window.id == window_id)
            else {
                return control_error("window_not_found", "window not found");
            };
            let Some(position) = window.tabs.iter().position(|tab| tab.id == tab_id) else {
                return control_error("tab_not_found", "tab not found");
            };

            let tab = window.tabs.remove(position);
            let session_id = terminal_session_id_for_tab(&tab);
            let browser_session_id = browser_session_id_for_tab(&tab);
            window.active_tab_id = window
                .active_tab_id
                .filter(|active_tab_id| *active_tab_id != tab_id)
                .or_else(|| window.tabs.first().map(|tab| tab.id));
            (session_id, browser_session_id)
        };

        if let Some(session_id) = session_id {
            self.remove_terminal_runtime(session_id);
        }
        if let Some(session_id) = browser_session_id {
            self.remove_browser_runtime(session_id);
        }
        self.active_workspace_index = index;
        self.workspaces[index].active_window_id = Some(window_id);
        self.push_event(
            "info",
            "window",
            format!("closed tab {tab_id:?} in window {window_id:?}"),
        );
        ControlResult::WindowTabClosed { window_id, tab_id }
    }

    fn move_window_tab(
        &mut self,
        source_window_id: WindowId,
        tab_id: TabId,
        target_window_id: WindowId,
    ) -> ControlResult {
        let Some(source_workspace_index) = self.workspace_index_for_window(source_window_id) else {
            return control_error("source_window_not_found", "source window not found");
        };
        let Some(target_workspace_index) = self.workspace_index_for_window(target_window_id) else {
            return control_error("target_window_not_found", "target window not found");
        };

        let moved_tab = {
            let Some(source_window) = self.workspaces[source_workspace_index]
                .windows
                .iter_mut()
                .find(|window| window.id == source_window_id)
            else {
                return control_error("source_window_not_found", "source window not found");
            };
            let Some(tab_position) = source_window.tabs.iter().position(|tab| tab.id == tab_id)
            else {
                return control_error("tab_not_found", "tab not found");
            };
            let tab = source_window.tabs.remove(tab_position);
            source_window.active_tab_id = source_window
                .active_tab_id
                .filter(|active_tab_id| *active_tab_id != tab_id)
                .or_else(|| source_window.tabs.first().map(|tab| tab.id));
            tab
        };

        let Some(target_window) = self.workspaces[target_workspace_index]
            .windows
            .iter_mut()
            .find(|window| window.id == target_window_id)
        else {
            return control_error("target_window_not_found", "target window not found");
        };
        target_window.push_tab(moved_tab);
        self.active_workspace_index = target_workspace_index;
        self.workspaces[target_workspace_index].active_window_id = Some(target_window_id);
        self.push_event(
            "info",
            "window",
            format!(
                "moved tab {tab_id:?} from window {source_window_id:?} to window {target_window_id:?}"
            ),
        );
        ControlResult::WindowTabMoved {
            tab_id,
            source_window_id,
            target_window_id,
        }
    }

    fn set_workspace_layout(
        &mut self,
        workspace_id: WorkspaceId,
        mode: LayoutMode,
    ) -> ControlResult {
        let Some(index) = self.workspace_index_by_id(workspace_id) else {
            return control_error("workspace_not_found", "workspace not found");
        };

        self.workspaces[index].layout.mode = mode.clone();
        self.active_workspace_index = index;
        self.push_event(
            "info",
            "workspace",
            format!("set workspace {workspace_id:?} layout to {mode:?}"),
        );
        ControlResult::WorkspaceLayoutSet { workspace_id, mode }
    }

    fn open_terminal_window(
        &mut self,
        workspace_id: ah_core::WorkspaceId,
        title: String,
        cwd: Option<PathBuf>,
    ) -> ControlResult {
        let Some(index) = self.workspace_index_by_id(workspace_id) else {
            return control_error("workspace_not_found", "workspace not found");
        };
        self.active_workspace_index = index;

        let cwd = cwd
            .or_else(|| self.workspaces[index].root.clone())
            .unwrap_or_else(|| PathBuf::from("/"));
        let terminal = TerminalRuntime::spawn(title.clone(), cwd, self.terminal_wake_tx.clone());
        let session_id = terminal.session_id();
        let window_id = {
            let workspace = &mut self.workspaces[index];
            workspace.push_window(WorkspaceWindow::new(title.clone()));
            let window = workspace
                .windows
                .last_mut()
                .expect("push_window should insert a workspace window");
            window.push_tab(WindowTab::terminal(title, session_id));
            window.id
        };
        let workspace = &self.workspaces[index];
        let window = workspace
            .windows
            .iter()
            .find(|window| window.id == window_id)
            .expect("window id was returned from a workspace window");
        let summary = WindowSummary::from_window(workspace, window);
        self.terminals.push(terminal);
        self.push_event(
            "info",
            "window",
            format!("opened terminal tab for session {session_id:?}"),
        );
        ControlResult::WindowOpened {
            workspace_id,
            window: summary,
        }
    }

    fn open_terminal_tab(
        &mut self,
        window_id: WindowId,
        title: String,
        cwd: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) -> ControlResult {
        let Some(session_id) = self.add_terminal_tab_to_window_with_cwd(window_id, title, cwd)
        else {
            return control_error("window_not_found", "window not found");
        };
        self.ensure_terminal_focus_handles(cx);
        let Some(window) = self.window_by_id(window_id) else {
            return control_error("window_not_found", "window not found");
        };
        let Some(tab) = window.active_tab() else {
            return control_error("tab_not_found", "created tab not found");
        };
        let tab = WindowTabSummary::from_tab(window, tab);
        self.push_event(
            "info",
            "window",
            format!("opened terminal tab for session {session_id:?} in window {window_id:?}"),
        );
        ControlResult::WindowTabOpened { window_id, tab }
    }

    fn forward_block(
        &mut self,
        source_session_id: SessionId,
        block_id: BlockId,
        target_session_id: SessionId,
    ) -> ControlResult {
        let Some(source_terminal) = self.terminal_by_session(source_session_id) else {
            return control_error("source_session_not_found", "source session not found");
        };
        let Some(source_block) = source_terminal
            .view
            .blocks
            .iter()
            .find(|row| row.block.id == block_id)
            .map(|row| row.block.clone())
        else {
            return control_error("block_not_found", "block not found");
        };
        let Some(target_terminal) = self.terminal_by_session_mut(target_session_id) else {
            return control_error("target_session_not_found", "target session not found");
        };

        let mut forwarded = Block::new(
            target_session_id,
            Actor::Agent {
                name: "AgentHouse-control".to_string(),
            },
            BlockKind::AgentOutput,
            format!(
                "Forwarded from {source_session_id:?} block {block_id:?}\n\n{}",
                clean_forwarded_block_text(&source_block.text)
            ),
        );
        forwarded.complete();
        let row = BlockRow {
            title: "Forwarded block".into(),
            block: forwarded,
        };
        let summary = BlockSummary::from_block(&row.block);
        target_terminal.view.blocks.insert(0, row);
        target_terminal.view.ring.update(
            RingState::Complete,
            format!("block forwarded from {source_session_id:?}"),
        );
        if let Some((workspace_id, window_id, tab_id)) =
            self.activate_tab_for_session(target_session_id)
        {
            self.push_event(
                "info",
                "window",
                format!(
                    "activated forwarded target tab {tab_id:?} in window {window_id:?} workspace {workspace_id:?}"
                ),
            );
        }
        self.push_event(
            "info",
            "session",
            format!(
                "forwarded block {block_id:?} from {source_session_id:?} to {target_session_id:?}"
            ),
        );
        ControlResult::BlockForwarded {
            source_session_id,
            target_session_id,
            block: summary,
        }
    }

    fn open_web_window(
        &mut self,
        workspace_id: ah_core::WorkspaceId,
        title: String,
        url: String,
        _cx: &mut Context<Self>,
    ) -> ControlResult {
        let Some(index) = self.workspace_index_by_id(workspace_id) else {
            return control_error("workspace_not_found", "workspace not found");
        };
        self.active_workspace_index = index;
        let browser = match BrowserRuntime::new_native(
            title.clone(),
            url.clone(),
            self.browser_wake_tx.clone(),
        ) {
            Ok(browser) => browser,
            Err(error) => return control_error("native_webview_unavailable", error),
        };
        let session_id = browser.session_id();
        let window_id = {
            let workspace = &mut self.workspaces[index];
            workspace.push_window(WorkspaceWindow::new(title.clone()));
            let window = workspace
                .windows
                .last_mut()
                .expect("push_window should insert a workspace window");
            window.push_tab(WindowTab::web_with_session(title, session_id, url));
            window.id
        };
        let workspace = &self.workspaces[index];
        let window = workspace
            .windows
            .iter()
            .find(|window| window.id == window_id)
            .expect("window id was returned from a workspace window");
        let summary = WindowSummary::from_window(workspace, window);
        self.browsers.push(browser);
        self.refresh_browser_session(session_id);
        self.push_event("info", "window", "opened browser tab");
        ControlResult::WindowOpened {
            workspace_id,
            window: summary,
        }
    }

    fn open_web_tab(
        &mut self,
        window_id: WindowId,
        title: String,
        url: String,
        cx: &mut Context<Self>,
    ) -> ControlResult {
        let Some(workspace_index) = self.workspace_index_for_window(window_id) else {
            return control_error("window_not_found", "window not found");
        };
        let browser = match BrowserRuntime::new_native(
            title.clone(),
            url.clone(),
            self.browser_wake_tx.clone(),
        ) {
            Ok(browser) => browser,
            Err(error) => return control_error("native_webview_unavailable", error),
        };
        let session_id = browser.session_id();
        let Some(window) = self.workspaces[workspace_index]
            .windows
            .iter_mut()
            .find(|window| window.id == window_id)
        else {
            return control_error("window_not_found", "window not found");
        };
        window.push_tab(WindowTab::web_with_session(title, session_id, url));
        self.active_workspace_index = workspace_index;
        self.workspaces[workspace_index].active_window_id = Some(window_id);
        self.browsers.push(browser);
        self.refresh_browser_session(session_id);
        self.ensure_browser_focus_handles(cx);
        let window = self
            .window_by_id(window_id)
            .expect("window id was found in workspace");
        let tab = window.active_tab().expect("created tab should be active");
        let tab = WindowTabSummary::from_tab(window, tab);
        self.push_event(
            "info",
            "window",
            format!("opened browser tab for session {session_id:?} in window {window_id:?}"),
        );
        ControlResult::WindowTabOpened { window_id, tab }
    }

    fn split_window(
        &mut self,
        window_id: WindowId,
        direction: SplitDirection,
        cx: &mut Context<Self>,
    ) -> ControlResult {
        let Some(workspace_index) = self.workspace_index_for_window(window_id) else {
            return control_error("window_not_found", "window not found");
        };
        let workspace_id = self.workspaces[workspace_index].id;
        self.workspaces[workspace_index].layout.mode = match direction {
            SplitDirection::Right => LayoutMode::Columns,
            SplitDirection::Down => LayoutMode::Grid,
        };
        let Some(new_window_id) = self.add_terminal_pane_to_workspace(workspace_index) else {
            return control_error("pane_limit_reached", "workspace pane limit reached");
        };
        self.ensure_terminal_focus_handles(cx);
        let workspace = &self.workspaces[workspace_index];
        let window = workspace
            .windows
            .iter()
            .find(|window| window.id == new_window_id)
            .expect("created split window should exist");
        let summary = WindowSummary::from_window(workspace, window);
        self.push_event(
            "info",
            "window",
            format!(
                "split {direction} from {window_id:?}; created terminal pane {new_window_id:?} in workspace {workspace_id:?}"
            ),
        );
        ControlResult::WindowSplit {
            workspace_id,
            source_window_id: window_id,
            window: summary,
        }
    }

    fn workspace_onboarding(
        &self,
        theme: AgentHouseTheme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .items_center()
            .justify_center()
            .bg(theme.app_bg)
            .px(px(36.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .max_w(px(460.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(54.0))
                            .rounded(px(14.0))
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.panel_bg)
                            .shadow(glass_shadow_sm())
                            .child(app_icon(AppIcon::FolderOpen, theme.text_muted, 25.0)),
                    )
                    .child(
                        div()
                            .mt(px(16.0))
                            .font_family(UI_FONT_SANS)
                            .text_size(px(28.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text)
                            .child("AgentHouse"),
                    )
                    .child(
                        div()
                            .mt(px(7.0))
                            .font_family(UI_FONT_SANS)
                            .text_size(px(13.0))
                            .text_color(theme.text_muted)
                            .text_center()
                            .line_height(px(20.0))
                            .child(self.ui_text(
                                "选择一个文件夹，开始你的 AgentHouse 工作区。",
                                "Select a folder to start your AgentHouse workspace.",
                            )),
                    )
                    .child(
                        command_button(
                            "workspace-onboarding-open-folder",
                            self.ui_text("打开文件夹", "Open Folder"),
                            theme,
                        )
                        .mt(px(18.0))
                        .w(px(148.0))
                        .text_center()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_workspace_folder_from_ui(window, cx);
                        })),
                    ),
            )
    }

    fn workspace_rail(&self, theme: AgentHouseTheme, cx: &mut Context<Self>) -> impl IntoElement {
        let pin_create_workspace = self.workspaces.len() > INLINE_WORKSPACE_CREATE_LIMIT;
        let mut rail = div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .w(px(WORKSPACE_RAIL_WIDTH_PX))
            .min_w(px(WORKSPACE_RAIL_WIDTH_PX))
            .min_h_0()
            .overflow_hidden()
            .bg(theme.rail_bg)
            .border_r_1()
            .border_color(theme.border);

        rail = rail.child(
            div()
                .flex_shrink_0()
                .h(px(GLASS_WORKSPACE_HEADER_EMPTY_H_PX)),
        );

        rail = rail.child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .h(px(GLASS_WORKSPACE_SEARCH_H_PX))
                .mx(px(GLASS_WORKSPACE_SEARCH_MARGIN_X_PX))
                .mb(px(GLASS_WORKSPACE_SEARCH_MARGIN_B_PX))
                .font_family(UI_FONT_SANS)
                .font_weight(FontWeight::SEMIBOLD)
                .text_size(px(GLASS_WORKSPACE_SEARCH_TEXT_SIZE_PX + 3.0))
                .text_color(theme.text)
                .child("AgentHouse"),
        );

        let mut workspace_items = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .px(px(GLASS_WORKSPACE_LIST_PADDING_X_PX))
            .map(scroll_y);

        for (index, workspace) in self.workspaces.iter().enumerate() {
            let active = index == self.active_workspace_index;
            let bg = if active {
                theme.active_bg
            } else {
                transparent_rgba()
            };
            let border = if active {
                theme.border
            } else {
                transparent_rgba()
            };
            let root = workspace
                .root
                .as_ref()
                .map(|path| workspace_root_label(path))
                .unwrap_or_else(|| self.ui_text("未选择文件夹", "no folder").to_string());
            let initials = workspace_initials(&workspace.name);
            let (avatar_bg, avatar_text) = workspace_avatar_colors(index, theme);
            let tab_count = workspace
                .windows
                .iter()
                .map(|window| window.tabs.len())
                .sum::<usize>();
            let meta = workspace_meta_label(workspace.windows.len(), tab_count, self.ui_language);

            workspace_items = workspace_items.child(
                div()
                    .id(("workspace", index))
                    .cursor_pointer()
                    .rounded(px(GLASS_WORKSPACE_CARD_RADIUS_PX))
                    .border_1()
                    .border_color(border)
                    .bg(bg)
                    .p(px(GLASS_WORKSPACE_CARD_PADDING_PX))
                    .mb(px(GLASS_WORKSPACE_CARD_MARGIN_B_PX))
                    .hover(move |style| {
                        style.bg(if active {
                            theme.active_bg
                        } else {
                            theme.hover_bg
                        })
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(GLASS_WORKSPACE_NAME_GAP_PX))
                            .mb(px(GLASS_WORKSPACE_NAME_MARGIN_B_PX))
                            .child(
                                div()
                                    .flex()
                                    .flex_shrink_0()
                                    .items_center()
                                    .justify_center()
                                    .size(px(GLASS_WORKSPACE_ICON_PX))
                                    .rounded(px(GLASS_WORKSPACE_ICON_RADIUS_PX))
                                    .bg(avatar_bg)
                                    .font_family(UI_FONT_SANS)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_size(px(GLASS_WORKSPACE_ICON_TEXT_SIZE_PX))
                                    .text_color(avatar_text)
                                    .child(initials),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .font_family(UI_FONT_SANS)
                                    .text_size(px(GLASS_WORKSPACE_NAME_TEXT_SIZE_PX))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.text)
                                    .line_clamp(1)
                                    .child(workspace.name.clone()),
                            ),
                    )
                    .child(
                        div()
                            .pl(px(GLASS_WORKSPACE_META_INDENT_PX))
                            .mb(px(GLASS_WORKSPACE_META_MARGIN_B_PX))
                            .font_family(UI_FONT_MONO)
                            .text_size(px(GLASS_WORKSPACE_META_TEXT_SIZE_PX))
                            .text_color(theme.text_subtle)
                            .line_clamp(1)
                            .child(root),
                    )
                    .child(
                        div()
                            .pl(px(GLASS_WORKSPACE_META_INDENT_PX))
                            .font_family(UI_FONT_SANS)
                            .text_size(px(GLASS_WORKSPACE_META_TEXT_SIZE_PX))
                            .text_color(theme.text_subtle)
                            .child(meta),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.activate_workspace(index, cx);
                        this.focus_active_tab(window, cx);
                    })),
            );
        }

        if !pin_create_workspace {
            workspace_items = workspace_items.child(
                div()
                    .px(px(
                        GLASS_WORKSPACE_FOOTER_PADDING_X_PX - GLASS_WORKSPACE_LIST_PADDING_X_PX
                    ))
                    .py(px(GLASS_WORKSPACE_FOOTER_PADDING_Y_PX))
                    .child(
                        new_workspace_button(
                            "create-workspace-inline",
                            self.ui_text("New Workspace", "New Workspace"),
                            theme,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_workspace_folder_from_ui(window, cx);
                            this.focus_active_tab(window, cx);
                        })),
                    ),
            );
        }

        rail = rail.child(workspace_items);
        if pin_create_workspace {
            rail = rail.child(
                div()
                    .flex_shrink_0()
                    .px(px(GLASS_WORKSPACE_FOOTER_PADDING_X_PX))
                    .py(px(GLASS_WORKSPACE_FOOTER_PADDING_Y_PX))
                    .border_t_1()
                    .border_color(theme.border)
                    .child(
                        new_workspace_button(
                            "create-workspace",
                            self.ui_text("New Workspace", "New Workspace"),
                            theme,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_workspace_folder_from_ui(window, cx);
                            this.focus_active_tab(window, cx);
                        })),
                    ),
            );
        }
        rail
    }

    fn window_board(
        &self,
        theme: AgentHouseTheme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let workspace = self.active_workspace();
        let pane_count = workspace.windows.len().max(1);
        let pane_cols = pane_grid_columns(pane_count, &workspace.layout.mode);
        let pane_rows = pane_grid_rows(pane_count, pane_cols);
        let mut pane_grid = div()
            .grid()
            .grid_cols(pane_cols)
            .grid_rows(pane_rows)
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .bg(theme.border)
            .p_0()
            .gap(px(1.0));

        for (index, workspace_window) in workspace.windows.iter().enumerate() {
            let window_id = workspace_window.id;
            let active = workspace.active_window_id == Some(window_id);
            let (frame_color, frame_width_px) = pane_window_frame(active, theme);

            let mut pane = div()
                .id(("window", index))
                .cursor_pointer()
                .relative()
                .border_1()
                .border_color(frame_color)
                .bg(theme.panel_bg)
                .flex()
                .flex_col()
                .size_full()
                .min_w_0()
                .min_h_0()
                .overflow_hidden()
                .shadow(vec![BoxShadow {
                    color: Hsla::from(frame_color),
                    offset: point(px(0.0), px(0.0)),
                    blur_radius: px(0.0),
                    spread_radius: px(frame_width_px),
                    inset: true,
                }])
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.activate_window(window_id, cx);
                    this.focus_active_tab_in_window(window_id, window, cx);
                }));

            pane = pane.child(self.window_tab_bar(workspace_window, theme, cx));
            pane = match workspace_window.active_tab() {
                Some(tab) => pane.child(self.window_tab_body(tab, theme, window, cx)),
                None => pane.child(empty_pane_body(theme)),
            };
            if !active {
                pane = pane.child(div().absolute().inset_0().bg(theme.inactive_pane_overlay));
            }

            pane_grid = pane_grid.child(pane);
        }

        pane_grid
    }

    fn window_tab_bar(
        &self,
        workspace_window: &WorkspaceWindow,
        theme: AgentHouseTheme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let tab_bar = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .h(px(WINDOW_TAB_HEIGHT_PX))
            .px(px(GLASS_TABBAR_PADDING_X_PX))
            .gap(px(GLASS_TABBAR_GAP_PX))
            .border_b_1()
            .border_color(
                if self.active_workspace().active_window_id == Some(workspace_window.id) {
                    theme.border_strong
                } else {
                    theme.border
                },
            )
            .bg(theme.tabbar_bg)
            .overflow_hidden();

        let mut tab_list = div()
            .flex()
            .flex_1()
            .min_w_0()
            .gap(px(GLASS_TABBAR_GAP_PX))
            .py(px(GLASS_TABBAR_TAB_PADDING_Y_PX))
            .map(scroll_x);

        for tab in &workspace_window.tabs {
            let window_id = workspace_window.id;
            let tab_id = tab.id;
            let active = workspace_window.active_tab_id == Some(tab.id);
            let terminal_session_id = terminal_session_id_for_tab(tab);
            let browser_session_id = browser_session_id_for_tab(tab);
            let tab_icon = tab_content_icon(&tab.content);
            let tab_group =
                SharedString::from(format!("window-tab-group-{window_id:?}-{tab_id:?}"));
            let bg = if active {
                theme.focus_bg
            } else {
                theme.tabbar_bg
            };
            let text = if active { theme.text } else { theme.text_muted };
            let hover_text = if active { text } else { theme.text_muted };

            tab_list = tab_list.child(
                div()
                    .id(format!("window-tab-{window_id:?}-{tab_id:?}"))
                    .cursor_pointer()
                    .group(tab_group.clone())
                    .rounded(px(GLASS_TAB_RADIUS_PX))
                    .bg(bg)
                    .px(px(GLASS_TAB_PADDING_X_PX))
                    .py(px(GLASS_TAB_PADDING_Y_PX))
                    .max_w(px(GLASS_TAB_MAX_W_PX))
                    .hover(move |style| style.bg(if active { bg } else { theme.hover_bg }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(GLASS_TAB_GAP_PX))
                            .min_w_0()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .opacity(0.6)
                                    .child(hoverable_app_icon(
                                        tab_icon,
                                        text,
                                        hover_text,
                                        WINDOW_TAB_ICON_SIZE_PX,
                                        tab_group.clone(),
                                    )),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .font_family(UI_FONT_SANS)
                                    .text_size(px(GLASS_TAB_TEXT_SIZE_PX))
                                    .font_weight(if active {
                                        FontWeight::MEDIUM
                                    } else {
                                        FontWeight(450.0)
                                    })
                                    .text_color(text)
                                    .group_hover(tab_group.clone(), move |style| {
                                        style.text_color(hover_text)
                                    })
                                    .line_clamp(1)
                                    .child(tab.title.clone()),
                            )
                            .child(
                                tab_close_button(
                                    format!("close-tab-{window_id:?}-{tab_id:?}"),
                                    tab_group.clone(),
                                    theme,
                                )
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        cx.stop_propagation();
                                        this.close_window_tab_from_ui(window_id, tab_id, cx);
                                        this.focus_active_terminal(window, cx);
                                    },
                                )),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.activate_window_tab(window_id, tab_id, cx);
                        if let Some(session_id) = terminal_session_id {
                            this.focus_terminal_session(session_id, window, cx);
                        } else if let Some(session_id) = browser_session_id {
                            this.focus_browser_content(session_id, window, cx);
                        }
                    })),
            );
        }

        let window_id = workspace_window.id;
        let controls = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(GLASS_PANE_ACTION_GROUP_GAP_PX))
            .ml(px(GLASS_PANE_ACTION_GROUP_MARGIN_L_PX))
            .pl(px(GLASS_PANE_ACTION_GROUP_PADDING_L_PX))
            .w(px(pane_action_group_width(GLASS_PANE_ACTION_COUNT)
                + GLASS_PANE_ACTION_GROUP_PADDING_L_PX))
            .border_l_1()
            .border_color(theme.border)
            .child(
                pane_icon_button(
                    format!("pane-new-terminal-{window_id:?}"),
                    AppIcon::Code,
                    Some(pane_action_tooltip(PaneAction::NewTerminal)),
                    theme,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    if let Some(session_id) = this.open_terminal_in_window_from_ui(window_id, cx) {
                        this.focus_terminal_session(session_id, window, cx);
                    }
                })),
            )
            .child(
                pane_icon_button(
                    format!("pane-new-browser-{window_id:?}"),
                    AppIcon::Web,
                    Some(pane_action_tooltip(PaneAction::NewBrowser)),
                    theme,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.open_browser_in_window_from_ui(window_id, window, cx);
                })),
            )
            .child(
                pane_icon_button(
                    format!("pane-split-right-{window_id:?}"),
                    AppIcon::SplitHorizontal,
                    Some(split_action_tooltip(SplitDirection::Right)),
                    theme,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    if let Some(new_window_id) =
                        this.split_window_from_ui(window_id, SplitDirection::Right, cx)
                    {
                        this.focus_active_tab_in_window(new_window_id, window, cx);
                    }
                })),
            )
            .child(
                pane_icon_button(
                    format!("pane-split-down-{window_id:?}"),
                    AppIcon::SplitVertical,
                    Some(split_action_tooltip(SplitDirection::Down)),
                    theme,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    if let Some(new_window_id) =
                        this.split_window_from_ui(window_id, SplitDirection::Down, cx)
                    {
                        this.focus_active_tab_in_window(new_window_id, window, cx);
                    }
                })),
            );

        tab_bar.child(tab_list).child(controls)
    }

    fn window_tab_body(
        &self,
        tab: &WindowTab,
        theme: AgentHouseTheme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match &tab.content {
            WindowContent::Terminal { session_id } => self
                .terminal_window_body(*session_id, theme, window, cx)
                .into_any_element(),
            WindowContent::Web { session_id, url } => self
                .web_window_body(*session_id, url, theme, window, cx)
                .into_any_element(),
            WindowContent::FilePreview { path } => {
                self.file_window_body(path, theme).into_any_element()
            }
        }
    }

    fn activate_window_tab(&mut self, window_id: WindowId, tab_id: TabId, cx: &mut Context<Self>) {
        if let Some(index) = self.workspace_index_for_window(window_id) {
            self.workspaces[index].active_window_id = Some(window_id);
        }
        if let Some(window) = self.window_by_id_mut(window_id)
            && window.activate_tab(tab_id)
        {
            self.push_event(
                "info",
                "window",
                format!("activated tab {tab_id:?} in window {window_id:?}"),
            );
            cx.notify();
        }
    }

    fn terminal_window_body(
        &self,
        session_id: SessionId,
        theme: AgentHouseTheme,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let Some(terminal) = self.terminal_by_session(session_id) else {
            return pane_state_body(
                self.ui_text("终端会话不存在", "terminal session is missing"),
                theme,
                true,
            );
        };
        let Some(focus_handle) = self.terminal_focus_handles.get(&session_id).cloned() else {
            return pane_state_body(
                self.ui_text("终端焦点目标不存在", "terminal focus target is missing"),
                theme,
                true,
            );
        };
        let session_title = terminal_surface_title(terminal);
        let session_status = terminal_surface_badge_label(terminal, self.ui_language);

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .bg(theme.terminal_bg)
            .p_0()
            .child(terminal_surface_header(
                session_title,
                session_status,
                theme,
            ))
            .child(self.terminal_grid_view(session_id, terminal, focus_handle, theme, cx))
    }

    #[allow(dead_code)]
    fn terminal_command_prompt(
        &self,
        session_id: SessionId,
        theme: AgentHouseTheme,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let input = self
            .terminal_command_inputs
            .get(&session_id)
            .cloned()
            .unwrap_or_default();
        let command_running = self
            .terminal_by_session(session_id)
            .is_some_and(TerminalRuntime::is_command_running);
        let placeholder = if command_running {
            "type stdin, press enter"
        } else {
            "command capture, press enter"
        };
        let display = if input.is_empty() {
            placeholder.to_string()
        } else {
            terminal_input_for_display(&input)
        };
        let text_color = if input.is_empty() {
            theme.terminal_placeholder
        } else {
            theme.text
        };
        let prompt = if command_running { ">" } else { "$" };

        div()
            .id(format!("terminal-command-input-{session_id:?}"))
            .track_focus(&self.terminal_command_focus)
            .cursor_pointer()
            .rounded_sm()
            .border_1()
            .border_color(if focused {
                theme.active_border
            } else {
                theme.border_strong
            })
            .bg(theme.terminal_input_bg)
            .px_2()
            .py_1()
            .min_h(px(30.0))
            .on_click(cx.listener(|this, _, window, cx| {
                window.focus(&this.terminal_command_focus, cx);
            }))
            .on_key_down(cx.listener(move |this, event, _window, cx| {
                this.handle_terminal_command_key(session_id, event, cx);
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .min_w_0()
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(11.0))
                            .text_color(theme.command_prompt)
                            .child(prompt),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .text_size(px(11.0))
                            .text_color(text_color)
                            .line_clamp(1)
                            .child(display),
                    ),
            )
    }

    fn terminal_grid_view(
        &self,
        session_id: SessionId,
        terminal: &TerminalRuntime,
        focus_handle: FocusHandle,
        theme: AgentHouseTheme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let snapshot = terminal.emulator.snapshot();
        let shell = cx.weak_entity();
        let click_focus_handle = focus_handle.clone();
        let grid = div()
            .id(format!("terminal-grid-{session_id:?}"))
            .track_focus(&focus_handle)
            .cursor_pointer()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_hidden()
            .border_0()
            .bg(theme.terminal_bg)
            .px(px(GLASS_TERMINAL_BODY_PADDING_X_PX))
            .py(px(GLASS_TERMINAL_BODY_PADDING_Y_PX))
            .font_family(UI_FONT_MONO)
            .text_size(px(TERMINAL_FONT_SIZE_PX))
            .line_height(px(TERMINAL_CELL_HEIGHT_PX))
            .on_click(cx.listener(move |_this, _, window, cx| {
                window.focus(&click_focus_handle, cx);
            }))
            .on_key_down(cx.listener(move |this, event, _window, cx| {
                this.write_terminal_key_for_session(session_id, event, cx);
                cx.stop_propagation();
            }))
            .child(TerminalScreenElement { snapshot, theme });

        TerminalGridSizer {
            session_id,
            shell,
            child: Some(grid.into_any_element()),
        }
    }

    #[allow(dead_code)]
    fn handle_terminal_command_key(
        &mut self,
        session_id: SessionId,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.modifiers.platform && event.keystroke.key.eq_ignore_ascii_case("v") {
            if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                self.terminal_command_inputs
                    .entry(session_id)
                    .or_default()
                    .push_str(&text);
            }
            cx.notify();
            cx.stop_propagation();
            return;
        }

        if event.keystroke.modifiers.control && !event.keystroke.modifiers.platform {
            match event.keystroke.key.as_str() {
                "c" => {
                    let buffer = self.terminal_command_inputs.entry(session_id).or_default();
                    if buffer.is_empty() {
                        self.interrupt_session_from_ui(session_id, cx);
                    } else {
                        buffer.clear();
                        cx.notify();
                    }
                    cx.stop_propagation();
                    return;
                }
                "d" => {
                    self.write_terminal_input_from_ui_for_session(session_id, "\x04", cx);
                    cx.stop_propagation();
                    return;
                }
                "u" => {
                    self.terminal_command_inputs
                        .entry(session_id)
                        .or_default()
                        .clear();
                    cx.notify();
                    cx.stop_propagation();
                    return;
                }
                _ => {}
            }
        }

        match event.keystroke.key.as_str() {
            "enter" => {
                let command_running = self
                    .terminal_by_session(session_id)
                    .is_some_and(TerminalRuntime::is_command_running);
                let input = self
                    .terminal_command_inputs
                    .remove(&session_id)
                    .unwrap_or_default();
                match terminal_prompt_submission(&input, command_running) {
                    Some(TerminalPromptSubmission::Command(command)) => {
                        self.run_terminal_command_for_session(session_id, command, cx);
                    }
                    Some(TerminalPromptSubmission::Stdin(input)) => {
                        self.write_terminal_input_from_ui_for_session(session_id, input, cx);
                    }
                    None => cx.notify(),
                }
            }
            "backspace" => {
                self.terminal_command_inputs
                    .entry(session_id)
                    .or_default()
                    .pop();
                cx.notify();
            }
            "escape" => {
                self.terminal_command_inputs
                    .entry(session_id)
                    .or_default()
                    .clear();
                cx.notify();
            }
            _ => {
                if !event.keystroke.modifiers.control
                    && !event.keystroke.modifiers.platform
                    && let Some(text) = event.keystroke.key_char.as_deref()
                    && !text.chars().any(char::is_control)
                {
                    self.terminal_command_inputs
                        .entry(session_id)
                        .or_default()
                        .push_str(text);
                    cx.notify();
                }
            }
        }
        cx.stop_propagation();
    }

    fn current_browser_address(&self, session_id: SessionId) -> String {
        self.browser_address_inputs
            .get(&session_id)
            .cloned()
            .unwrap_or_else(|| {
                self.browser_by_session(session_id)
                    .map(|browser| browser.state.current_url.clone())
                    .unwrap_or_default()
            })
    }

    fn browser_address_render_state(&self, session_id: SessionId) -> BrowserAddressRenderState {
        let text = self.current_browser_address(session_id);
        let mut state = self
            .browser_address_edits
            .get(&session_id)
            .map(|edit| BrowserAddressRenderState {
                selected_range: edit.selected_range.clone(),
                selection_reversed: edit.selection_reversed,
                marked_range: edit.marked_range.clone(),
                cursor_offset: edit.cursor_offset(),
            })
            .unwrap_or_default();
        state.selected_range.start = clamp_to_char_boundary(&text, state.selected_range.start);
        state.selected_range.end = clamp_to_char_boundary(&text, state.selected_range.end);
        state.cursor_offset = clamp_to_char_boundary(&text, state.cursor_offset);
        state.marked_range = state.marked_range.and_then(|range| {
            let start = clamp_to_char_boundary(&text, range.start);
            let end = clamp_to_char_boundary(&text, range.end);
            (start <= end).then_some(start..end)
        });
        state
    }

    fn ensure_browser_address_edit(&mut self, session_id: SessionId) {
        let text = self.current_browser_address(session_id);
        self.browser_address_inputs
            .entry(session_id)
            .or_insert_with(|| text.clone());
        let edit = self.browser_address_edits.entry(session_id).or_default();
        edit.clamp_to_text(&text);
    }

    fn focus_browser_address_for_edit(&mut self, session_id: SessionId, address: Option<String>) {
        let address = address.unwrap_or_else(|| self.current_browser_address(session_id));
        self.browser_address_inputs
            .insert(session_id, address.clone());
        let edit = self.browser_address_edits.entry(session_id).or_default();
        edit.select_all(&address);
        edit.clear_layout();
    }

    fn cancel_browser_address_edit(&mut self, session_id: SessionId, cx: &mut Context<Self>) {
        self.browser_address_inputs.remove(&session_id);
        self.browser_address_edits.remove(&session_id);
        cx.notify();
    }

    fn clear_browser_address_edit(&mut self, session_id: SessionId) {
        self.browser_address_inputs.remove(&session_id);
        self.browser_address_edits.remove(&session_id);
    }

    fn select_browser_address_all(&mut self, session_id: SessionId, cx: &mut Context<Self>) {
        self.ensure_browser_address_edit(session_id);
        let text = self.current_browser_address(session_id);
        if let Some(edit) = self.browser_address_edits.get_mut(&session_id) {
            edit.select_all(&text);
        }
        cx.notify();
    }

    fn move_browser_address_cursor(
        &mut self,
        session_id: SessionId,
        offset: usize,
        extend_selection: bool,
        cx: &mut Context<Self>,
    ) {
        self.ensure_browser_address_edit(session_id);
        let text = self.current_browser_address(session_id);
        let offset = clamp_to_char_boundary(&text, offset);
        if let Some(edit) = self.browser_address_edits.get_mut(&session_id) {
            if extend_selection {
                browser_address_select_to(edit, offset);
            } else {
                edit.move_to(offset);
            }
        }
        cx.notify();
    }

    fn copy_browser_address_selection(&mut self, session_id: SessionId, cx: &mut Context<Self>) {
        self.ensure_browser_address_edit(session_id);
        let text = self.current_browser_address(session_id);
        let Some(edit) = self.browser_address_edits.get(&session_id) else {
            return;
        };
        if edit.selected_range.is_empty() {
            return;
        }
        let range = edit.selected_range.clone();
        if range.end <= text.len() {
            cx.write_to_clipboard(ClipboardItem::new_string(text[range].to_string()));
        }
    }

    fn replace_browser_address_text(
        &mut self,
        session_id: SessionId,
        range: Option<Range<usize>>,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        self.ensure_browser_address_edit(session_id);
        let mut address = self.current_browser_address(session_id);
        let Some(edit) = self.browser_address_edits.get_mut(&session_id) else {
            return;
        };
        edit.clamp_to_text(&address);
        let range = range
            .or_else(|| edit.marked_range.clone())
            .unwrap_or_else(|| edit.selected_range.clone());
        let range = clamp_range_to_text(&address, range);
        let text = sanitize_browser_address_insert_text(text);
        address.replace_range(range.clone(), &text);
        let cursor = range.start + text.len();
        edit.move_to(cursor);
        edit.marked_range = None;
        self.browser_address_inputs.insert(session_id, address);
        cx.notify();
    }

    fn replace_browser_address_text_utf16(
        &mut self,
        session_id: SessionId,
        range_utf16: Option<Range<usize>>,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        let address = self.current_browser_address(session_id);
        let range = range_utf16
            .as_ref()
            .map(|range| range_from_utf16(&address, range));
        self.replace_browser_address_text(session_id, range, text, cx);
    }

    fn replace_and_mark_browser_address_text_utf16(
        &mut self,
        session_id: SessionId,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        cx: &mut Context<Self>,
    ) {
        self.ensure_browser_address_edit(session_id);
        let mut address = self.current_browser_address(session_id);
        let Some(edit) = self.browser_address_edits.get_mut(&session_id) else {
            return;
        };
        edit.clamp_to_text(&address);
        let range = range_utf16
            .as_ref()
            .map(|range| range_from_utf16(&address, range))
            .or_else(|| edit.marked_range.clone())
            .unwrap_or_else(|| edit.selected_range.clone());
        let range = clamp_range_to_text(&address, range);
        let new_text = sanitize_browser_address_insert_text(new_text);
        address.replace_range(range.clone(), &new_text);
        let marked_range =
            (!new_text.is_empty()).then_some(range.start..range.start + new_text.len());
        let selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range| range_from_utf16(&new_text, range))
            .map(|new_range| range.start + new_range.start..range.start + new_range.end)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());
        edit.selected_range = clamp_range_to_text(&address, selected_range);
        edit.selection_reversed = false;
        edit.marked_range = marked_range;
        self.browser_address_inputs.insert(session_id, address);
        cx.notify();
    }

    fn unmark_browser_address_text(&mut self, session_id: SessionId, cx: &mut Context<Self>) {
        if let Some(edit) = self.browser_address_edits.get_mut(&session_id) {
            edit.marked_range = None;
            cx.notify();
        }
    }

    fn delete_browser_address_backward(&mut self, session_id: SessionId, cx: &mut Context<Self>) {
        self.ensure_browser_address_edit(session_id);
        let text = self.current_browser_address(session_id);
        let Some(edit) = self.browser_address_edits.get(&session_id) else {
            return;
        };
        let range = if edit.selected_range.is_empty() {
            previous_text_boundary(&text, edit.cursor_offset())..edit.cursor_offset()
        } else {
            edit.selected_range.clone()
        };
        self.replace_browser_address_text(session_id, Some(range), "", cx);
    }

    fn delete_browser_address_forward(&mut self, session_id: SessionId, cx: &mut Context<Self>) {
        self.ensure_browser_address_edit(session_id);
        let text = self.current_browser_address(session_id);
        let Some(edit) = self.browser_address_edits.get(&session_id) else {
            return;
        };
        let range = if edit.selected_range.is_empty() {
            edit.cursor_offset()..next_text_boundary(&text, edit.cursor_offset())
        } else {
            edit.selected_range.clone()
        };
        self.replace_browser_address_text(session_id, Some(range), "", cx);
    }

    fn browser_address_text_for_range(
        &self,
        session_id: SessionId,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
    ) -> Option<String> {
        let text = self.current_browser_address(session_id);
        let range = range_from_utf16(&text, &range_utf16);
        let range = clamp_range_to_text(&text, range);
        adjusted_range.replace(range_to_utf16(&text, &range));
        Some(text[range].to_string())
    }

    fn browser_address_selection_utf16(&self, session_id: SessionId) -> UTF16Selection {
        let text = self.current_browser_address(session_id);
        let state = self.browser_address_render_state(session_id);
        UTF16Selection {
            range: range_to_utf16(&text, &state.selected_range),
            reversed: state.selection_reversed,
        }
    }

    fn browser_address_marked_range_utf16(&self, session_id: SessionId) -> Option<Range<usize>> {
        let text = self.current_browser_address(session_id);
        self.browser_address_render_state(session_id)
            .marked_range
            .as_ref()
            .map(|range| range_to_utf16(&text, range))
    }

    fn browser_address_bounds_for_range(
        &self,
        session_id: SessionId,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
    ) -> Bounds<Pixels> {
        let text = self.current_browser_address(session_id);
        let range = range_from_utf16(&text, &range_utf16);
        let Some(edit) = self.browser_address_edits.get(&session_id) else {
            return element_bounds;
        };
        let Some(line) = edit.last_layout.as_ref() else {
            return element_bounds;
        };
        let range = clamp_range_to_text(&text, range);
        Bounds::from_corners(
            Point::new(
                element_bounds.origin.x + line.x_for_index(range.start),
                element_bounds.origin.y,
            ),
            Point::new(
                element_bounds.origin.x + line.x_for_index(range.end),
                element_bounds.origin.y + element_bounds.size.height,
            ),
        )
    }

    fn browser_address_index_for_point(
        &self,
        session_id: SessionId,
        point: Point<Pixels>,
    ) -> Option<usize> {
        let text = self.current_browser_address(session_id);
        if text.is_empty() {
            return Some(0);
        }
        let edit = self.browser_address_edits.get(&session_id)?;
        let bounds = edit.last_bounds.as_ref()?;
        let line = edit.last_layout.as_ref()?;
        let local = bounds.localize(&point)?;
        let index = line.closest_index_for_x(local.x);
        Some(offset_to_utf16(&text, clamp_to_char_boundary(&text, index)))
    }

    fn handle_browser_address_key(
        &mut self,
        session_id: SessionId,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) {
        self.ensure_browser_address_edit(session_id);

        if event.keystroke.modifiers.platform {
            match event.keystroke.key.to_ascii_lowercase().as_str() {
                "a" => self.select_browser_address_all(session_id, cx),
                "c" => self.copy_browser_address_selection(session_id, cx),
                "x" => {
                    self.copy_browser_address_selection(session_id, cx);
                    let range = self
                        .browser_address_edits
                        .get(&session_id)
                        .map(|edit| edit.selected_range.clone())
                        .unwrap_or_default();
                    if !range.is_empty() {
                        self.replace_browser_address_text(session_id, Some(range), "", cx);
                    }
                }
                "v" => {
                    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                        self.replace_browser_address_text(session_id, None, text.trim(), cx);
                    }
                }
                "l" => self.select_browser_address_all(session_id, cx),
                _ => return,
            }
            cx.stop_propagation();
            return;
        }

        if event.keystroke.modifiers.control {
            return;
        }

        let text = self.current_browser_address(session_id);
        let cursor = self
            .browser_address_edits
            .get(&session_id)
            .map(BrowserAddressEditState::cursor_offset)
            .unwrap_or(0);
        let extend_selection = event.keystroke.modifiers.shift;

        match event.keystroke.key.as_str() {
            "enter" => {
                let input = self.current_browser_address(session_id);
                self.navigate_browser_from_ui(session_id, input, cx);
            }
            "backspace" => self.delete_browser_address_backward(session_id, cx),
            "delete" => self.delete_browser_address_forward(session_id, cx),
            "escape" => {
                self.cancel_browser_address_edit(session_id, cx);
            }
            "left" => {
                self.move_browser_address_cursor(
                    session_id,
                    previous_text_boundary(&text, cursor),
                    extend_selection,
                    cx,
                );
            }
            "right" => {
                self.move_browser_address_cursor(
                    session_id,
                    next_text_boundary(&text, cursor),
                    extend_selection,
                    cx,
                );
            }
            "home" => self.move_browser_address_cursor(session_id, 0, extend_selection, cx),
            "end" => {
                self.move_browser_address_cursor(session_id, text.len(), extend_selection, cx);
            }
            _ => {
                if event.keystroke.key_char.is_some() {
                    return;
                }
                return;
            }
        }
        cx.stop_propagation();
    }

    fn handle_browser_content_key(
        &mut self,
        session_id: SessionId,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.modifiers.platform && event.keystroke.key.eq_ignore_ascii_case("v") {
            if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                self.send_browser_input_from_ui(session_id, BrowserInput::KeyText { text }, cx);
            }
            cx.stop_propagation();
            return;
        }

        if event.keystroke.modifiers.control || event.keystroke.modifiers.platform {
            return;
        }

        let input = match event.keystroke.key.as_str() {
            "enter" => Some(BrowserInput::KeyPress {
                key: "Enter".to_string(),
            }),
            "backspace" => Some(BrowserInput::KeyPress {
                key: "Backspace".to_string(),
            }),
            "tab" => Some(BrowserInput::KeyPress {
                key: "Tab".to_string(),
            }),
            "escape" => Some(BrowserInput::KeyPress {
                key: "Escape".to_string(),
            }),
            "left" => Some(BrowserInput::KeyPress {
                key: "ArrowLeft".to_string(),
            }),
            "right" => Some(BrowserInput::KeyPress {
                key: "ArrowRight".to_string(),
            }),
            "up" => Some(BrowserInput::KeyPress {
                key: "ArrowUp".to_string(),
            }),
            "down" => Some(BrowserInput::KeyPress {
                key: "ArrowDown".to_string(),
            }),
            _ => event
                .keystroke
                .key_char
                .as_ref()
                .filter(|text| !text.chars().any(char::is_control))
                .map(|text| BrowserInput::KeyText { text: text.clone() }),
        };

        if let Some(input) = input {
            self.send_browser_input_from_ui(session_id, input, cx);
            cx.stop_propagation();
        }
    }

    fn web_window_body(
        &self,
        session_id: SessionId,
        url: &str,
        theme: AgentHouseTheme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let browser = self.browser_by_session(session_id);
        let preview = self.web_preview_for_session(session_id, url);
        let has_error = preview.error.is_some();
        let load_status = browser
            .map(|browser| browser_load_status_label(&browser.state.status, self.ui_language))
            .unwrap_or_else(|| self.ui_text("缺失", "Missing"));
        let preview_status = browser_preview_status_label(&preview, self.ui_language);
        let status_pill = if has_error {
            preview_status
        } else {
            load_status.to_string()
        };
        let address = self
            .browser_address_inputs
            .get(&session_id)
            .cloned()
            .unwrap_or_else(|| {
                browser
                    .map(|browser| browser.state.current_url.clone())
                    .unwrap_or_else(|| url.to_string())
            });
        let address_focused = self
            .browser_address_focus_handles
            .get(&session_id)
            .is_some_and(|focus_handle| focus_handle.is_focused(window));
        let content_focus_handle = self
            .browser_content_focus_handles
            .get(&session_id)
            .expect("browser content focus handles are ensured")
            .clone();
        let address_state = self.browser_address_render_state(session_id);
        let frame = browser.and_then(|browser| browser.frame.as_ref());
        let shell = cx.weak_entity();
        let back_session_id = session_id;
        let forward_session_id = session_id;
        let reload_session_id = session_id;

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .bg(theme.panel_bg)
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(GLASS_BROWSER_ADDRESS_GAP_PX))
                    .min_w_0()
                    .px(px(GLASS_BROWSER_ADDRESS_PADDING_X_PX))
                    .py(px(GLASS_BROWSER_ADDRESS_PADDING_Y_PX))
                    .border_b_1()
                    .border_color(theme.border)
                    .bg(theme.panel_alt_bg)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(GLASS_BROWSER_NAV_GROUP_GAP_PX))
                            .flex_shrink_0()
                            .child(
                                browser_nav_button("browser-back", "<", "Back", theme).on_click(
                                    cx.listener(move |this, _, _window, cx| {
                                        cx.stop_propagation();
                                        this.apply_browser_action_from_ui(
                                            back_session_id,
                                            BrowserAction::Back,
                                            cx,
                                        );
                                    }),
                                ),
                            )
                            .child(
                                browser_nav_button("browser-forward", ">", "Forward", theme)
                                    .on_click(cx.listener(move |this, _, _window, cx| {
                                        cx.stop_propagation();
                                        this.apply_browser_action_from_ui(
                                            forward_session_id,
                                            BrowserAction::Forward,
                                            cx,
                                        );
                                    })),
                            )
                            .child(
                                browser_nav_button("browser-reload", "R", "Reload", theme)
                                    .on_click(cx.listener(move |this, _, _window, cx| {
                                        cx.stop_propagation();
                                        this.apply_browser_action_from_ui(
                                            reload_session_id,
                                            BrowserAction::Reload,
                                            cx,
                                        );
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .id(format!("browser-address-{session_id:?}"))
                            .track_focus(
                                self.browser_address_focus_handles
                                    .get(&session_id)
                                    .expect("browser address focus handles are ensured"),
                            )
                            .cursor(CursorStyle::IBeam)
                            .flex()
                            .items_center()
                            .gap_2()
                            .flex_1()
                            .min_w_0()
                            .h(px(GLASS_BROWSER_ADDRESS_H_PX))
                            .rounded(px(GLASS_RADIUS_SM_PX))
                            .border_1()
                            .border_color(if address_focused {
                                theme.border_strong
                            } else {
                                theme.border
                            })
                            .bg(if address_focused {
                                theme.panel_bg
                            } else {
                                theme.panel_inset_bg
                            })
                            .px(px(GLASS_BROWSER_ADDRESS_INPUT_PADDING_X_PX))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if let Some(focus_handle) =
                                    this.browser_address_focus_handles.get(&session_id)
                                {
                                    window.focus(focus_handle, cx);
                                }
                                this.focus_browser_address_for_edit(session_id, None);
                                cx.notify();
                            }))
                            .on_key_down(cx.listener(move |this, event, _window, cx| {
                                this.handle_browser_address_key(session_id, event, cx);
                            }))
                            .child(BrowserAddressElement {
                                session_id,
                                shell,
                                text: address,
                                placeholder: browser_address_placeholder(self.ui_language),
                                state: address_state,
                                focused: address_focused,
                                theme,
                            }),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap(px(GLASS_BROWSER_STATUS_GAP_PX))
                            .max_w(px(GLASS_BROWSER_STATUS_MAX_W_PX))
                            .rounded_full()
                            .px(px(GLASS_BROWSER_STATUS_PADDING_X_PX))
                            .py(px(GLASS_BROWSER_STATUS_PADDING_Y_PX))
                            .bg(if has_error {
                                theme.tag_red_bg
                            } else {
                                theme.tag_green_bg
                            })
                            .font_family(UI_FONT_MONO)
                            .text_size(px(GLASS_BROWSER_STATUS_TEXT_SIZE_PX))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(if has_error {
                                theme.tag_red_text
                            } else {
                                theme.tag_green_text
                            })
                            .child(
                                div()
                                    .size(px(GLASS_BROWSER_STATUS_DOT_PX))
                                    .rounded_full()
                                    .bg(if has_error {
                                        theme.tag_red_text
                                    } else {
                                        theme.tag_green_text
                                    }),
                            )
                            .child(status_pill),
                    ),
            )
            .child(self.web_content_body(
                session_id,
                &preview,
                frame,
                content_focus_handle,
                theme,
                cx,
            ))
    }

    fn file_window_body(&self, path: &Path, theme: AgentHouseTheme) -> impl IntoElement {
        let preview = file_preview_snapshot(path);
        let content = preview
            .text
            .clone()
            .unwrap_or_else(|| preview.status.clone());
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .rounded_sm()
            .border_1()
            .border_color(theme.border)
            .bg(theme.panel_inset_bg)
            .px_2()
            .py_2()
            .gap_1()
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .min_w_0()
                    .child(
                        div()
                            .min_w_0()
                            .text_size(px(12.0))
                            .text_color(theme.text)
                            .line_clamp(1)
                            .child(path.display().to_string()),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(11.0))
                            .text_color(theme.text_muted)
                            .child(preview.status),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .rounded_sm()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.panel_bg)
                    .px_2()
                    .py_2()
                    .map(scroll_y)
                    .font_family("Menlo")
                    .text_size(px(11.0))
                    .text_color(theme.text_muted)
                    .child(content),
            )
    }
}

impl Render for AgentHouseShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_terminal_focus_handles(cx);
        self.ensure_browser_focus_handles(cx);
        let theme = AgentHouseTheme::for_scheme(self.ui_theme_scheme);

        let root = div()
            .flex()
            .size_full()
            .overflow_hidden()
            .bg(theme.app_bg)
            .font_family(UI_FONT_SANS)
            .on_action(cx.listener(|this, _: &OpenSettings, _window, cx| {
                this.push_event("info", "settings", "settings panel is reserved");
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &SetLanguageChinese, _window, cx| {
                this.set_ui_language_from_ui(UiLanguage::ZhCn, cx);
            }))
            .on_action(cx.listener(|this, _: &SetLanguageEnglish, _window, cx| {
                this.set_ui_language_from_ui(UiLanguage::En, cx);
            }))
            .on_action(cx.listener(|this, _: &SetThemeCream, _window, cx| {
                this.set_ui_theme_scheme_from_ui(UiThemeSchemePreference::Cream, cx);
            }))
            .on_action(cx.listener(|this, _: &SetThemeWarm, _window, cx| {
                this.set_ui_theme_scheme_from_ui(UiThemeSchemePreference::Warm, cx);
            }))
            .on_action(cx.listener(|this, _: &SetThemeBlue, _window, cx| {
                this.set_ui_theme_scheme_from_ui(UiThemeSchemePreference::Blue, cx);
            }))
            .on_action(cx.listener(|this, _: &SetThemeGreen, _window, cx| {
                this.set_ui_theme_scheme_from_ui(UiThemeSchemePreference::Green, cx);
            }))
            .on_action(cx.listener(|this, _: &SetThemeRed, _window, cx| {
                this.set_ui_theme_scheme_from_ui(UiThemeSchemePreference::Red, cx);
            }))
            .on_action(cx.listener(|this, _: &SetThemePurple, _window, cx| {
                this.set_ui_theme_scheme_from_ui(UiThemeSchemePreference::Purple, cx);
            }))
            .on_action(cx.listener(|this, _: &SetThemeGlass, _window, cx| {
                this.set_ui_theme_scheme_from_ui(UiThemeSchemePreference::Glass, cx);
            }))
            .on_action(cx.listener(|this, _: &SetThemeLuxury, _window, cx| {
                this.set_ui_theme_scheme_from_ui(UiThemeSchemePreference::Luxury, cx);
            }))
            .on_action(cx.listener(|this, _: &SetThemeSoft, _window, cx| {
                this.set_ui_theme_scheme_from_ui(UiThemeSchemePreference::Soft, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenWorkspaceFolder, window, cx| {
                this.open_workspace_folder_from_ui(window, cx);
            }))
            .on_action(cx.listener(|this, _: &RenameWorkspace, _window, cx| {
                this.begin_active_workspace_rename_from_menu(cx);
            }))
            .on_action(cx.listener(|this, _: &CloseWorkspace, _window, cx| {
                this.close_active_workspace_from_menu(cx);
            }))
            .on_action(cx.listener(|this, _: &NewTerminalTab, window, cx| {
                this.open_terminal_from_menu(window, cx);
            }))
            .on_action(cx.listener(|this, _: &NewWebTab, window, cx| {
                this.open_browser_from_menu(window, cx);
            }))
            .on_action(cx.listener(|this, _: &SplitWindowRight, window, cx| {
                this.split_window_from_menu(SplitDirection::Right, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SplitWindowDown, window, cx| {
                this.split_window_from_menu(SplitDirection::Down, window, cx);
            }));

        if self.workspaces.is_empty() {
            return root.child(self.workspace_onboarding(theme, cx));
        }

        root.child(self.workspace_rail(theme, cx))
            .child(self.window_board(theme, window, cx))
    }
}

fn completed_block(
    session_id: SessionId,
    actor: Actor,
    kind: BlockKind,
    title: impl Into<SharedString>,
    text: impl Into<String>,
) -> BlockRow {
    let mut block = Block::new(session_id, actor, kind, text);
    block.complete();
    BlockRow {
        title: title.into(),
        block,
    }
}

fn block_title_for_restore(block: &Block) -> SharedString {
    let first_line = block.text.lines().next().unwrap_or("restored block").trim();
    if first_line.is_empty() {
        block_kind_label(&block.kind).into()
    } else {
        format!("{}: {first_line}", block_kind_label(&block.kind)).into()
    }
}

fn command_button(
    id: impl Into<ElementId>,
    label: &'static str,
    theme: AgentHouseTheme,
) -> Stateful<Div> {
    div()
        .id(id)
        .cursor_pointer()
        .rounded_sm()
        .border_1()
        .border_color(theme.border_strong)
        .bg(theme.panel_alt_bg)
        .px_2()
        .py_1()
        .font_family(UI_FONT_SANS)
        .text_size(px(11.0))
        .text_color(theme.text)
        .child(label)
}

fn new_workspace_button(
    id: impl Into<ElementId>,
    label: &'static str,
    theme: AgentHouseTheme,
) -> Stateful<Div> {
    div()
        .id(id)
        .cursor_pointer()
        .flex()
        .items_center()
        .gap(px(GLASS_NEW_WORKSPACE_GAP_PX))
        .w(relative(1.0))
        .rounded(px(GLASS_RADIUS_MD_PX))
        .border_1()
        .border_dashed()
        .border_color(theme.border_strong)
        .bg(transparent_rgba())
        .px(px(GLASS_NEW_WORKSPACE_PADDING_X_PX))
        .py(px(GLASS_NEW_WORKSPACE_PADDING_Y_PX))
        .font_family(UI_FONT_SANS)
        .text_size(px(GLASS_NEW_WORKSPACE_TEXT_SIZE_PX))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.text_muted)
        .hover(move |mut style| {
            style.border_style = Some(BorderStyle::Solid);
            style.bg(theme.hover_bg).text_color(theme.text)
        })
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(px(GLASS_NEW_WORKSPACE_PLUS_PX))
                .rounded(px(GLASS_NEW_WORKSPACE_PLUS_RADIUS_PX))
                .bg(theme.active_bg)
                .text_size(px(GLASS_NEW_WORKSPACE_PLUS_TEXT_SIZE_PX))
                .text_color(theme.text_muted)
                .child("+"),
        )
        .child(label)
}

fn tab_close_button(
    id: impl Into<ElementId>,
    group: impl Into<SharedString>,
    theme: AgentHouseTheme,
) -> Stateful<Div> {
    div()
        .id(id)
        .cursor_pointer()
        .flex()
        .items_center()
        .justify_center()
        .size(px(GLASS_TAB_CLOSE_PX))
        .rounded(px(GLASS_PANE_ACTION_RADIUS_PX))
        .font_family(UI_FONT_SANS)
        .text_size(px(GLASS_TAB_CLOSE_TEXT_SIZE_PX))
        .text_color(theme.text_subtle)
        .opacity(0.0)
        .group_hover(group, |style| style.opacity(0.7))
        .hover(move |style| style.bg(theme.hover_bg).text_color(theme.text).opacity(1.0))
        .child("x")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PaneAction {
    NewTerminal,
    NewBrowser,
}

fn pane_action_tooltip(action: PaneAction) -> &'static str {
    match action {
        PaneAction::NewTerminal => "New Terminal",
        PaneAction::NewBrowser => "New Browser",
    }
}

fn pane_action_group_width(action_count: usize) -> f32 {
    if action_count == 0 {
        return 0.0;
    }
    PANE_ACTION_SIZE_PX * action_count as f32
        + GLASS_PANE_ACTION_GROUP_GAP_PX * action_count.saturating_sub(1) as f32
}

fn split_action_tooltip(direction: SplitDirection) -> &'static str {
    match direction {
        SplitDirection::Right => "Split Horizontal",
        SplitDirection::Down => "Split Vertical",
    }
}

struct GlassTooltip {
    label: SharedString,
    theme: AgentHouseTheme,
}

impl Render for GlassTooltip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .rounded(px(GLASS_RADIUS_SM_PX))
            .border_1()
            .border_color(self.theme.border)
            .bg(self.theme.panel_bg)
            .shadow(glass_shadow_sm())
            .px(px(GLASS_TOOLTIP_PADDING_X_PX))
            .py(px(GLASS_TOOLTIP_PADDING_Y_PX))
            .font_family(UI_FONT_SANS)
            .text_size(px(GLASS_TOOLTIP_TEXT_SIZE_PX))
            .font_weight(FontWeight::MEDIUM)
            .text_color(self.theme.text_muted)
            .child(self.label.clone())
    }
}

fn glass_tooltip(
    label: &'static str,
    theme: AgentHouseTheme,
) -> impl Fn(&mut Window, &mut App) -> AnyView {
    move |_window, cx| {
        cx.new(|_| GlassTooltip {
            label: SharedString::from(label),
            theme,
        })
        .into()
    }
}

fn glass_shadow_sm() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: Hsla::from(rgba(0x0000000a)),
        offset: point(px(0.0), px(1.0)),
        blur_radius: px(2.0),
        spread_radius: px(0.0),
        inset: false,
    }]
}

fn browser_nav_button(
    id: impl Into<ElementId>,
    label: &'static str,
    tooltip: &'static str,
    theme: AgentHouseTheme,
) -> Stateful<Div> {
    div()
        .id(id)
        .cursor_pointer()
        .flex()
        .items_center()
        .justify_center()
        .size(px(GLASS_BROWSER_NAV_SIZE_PX))
        .rounded(px(GLASS_BROWSER_NAV_RADIUS_PX))
        .font_family(UI_FONT_SANS)
        .text_size(px(GLASS_BROWSER_NAV_TEXT_SIZE_PX))
        .text_color(theme.text_subtle)
        .hover(move |style| style.bg(theme.hover_bg).text_color(theme.text))
        .tooltip(glass_tooltip(tooltip, theme))
        .child(label)
}

fn browser_address_placeholder(language: UiLanguage) -> &'static str {
    language.select("输入网址", "Enter URL")
}

fn browser_load_status_label(status: &BrowserLoadStatus, language: UiLanguage) -> &'static str {
    match status {
        BrowserLoadStatus::Idle => language.select("空闲", "Idle"),
        BrowserLoadStatus::Loading => language.select("加载中", "Loading"),
        BrowserLoadStatus::Loaded => language.select("已加载", "Loaded"),
        BrowserLoadStatus::Failed => language.select("失败", "Failed"),
    }
}

fn browser_preview_status_label(preview: &WebPreviewSnapshot, language: UiLanguage) -> String {
    if preview.error.is_some() {
        return language.select("失败", "Failed").to_string();
    }
    if preview.byte_count.is_some() {
        language.select("就绪", "Ready").to_string()
    } else {
        language.select("等待", "Pending").to_string()
    }
}

fn browser_preview_page(
    preview: &WebPreviewSnapshot,
    has_error: bool,
    theme: AgentHouseTheme,
    language: UiLanguage,
) -> Div {
    let title = if has_error {
        language.select("浏览器加载失败", "Browser Load Failed")
    } else {
        preview
            .title
            .as_deref()
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| language.select("浏览器预览", "Browser Preview"))
    };
    let subtitle = if let Some(error) = &preview.error {
        error.clone()
    } else if preview.text.is_none() {
        language
            .select(
                "浏览器后端暂时没有返回内容。",
                "The browser backend has not returned content yet.",
            )
            .to_string()
    } else {
        preview
            .text
            .as_deref()
            .map(|text| tail_chars(text, 180).replace('\n', " "))
            .filter(|text| !text.trim().is_empty())
            .unwrap_or_else(|| {
                language
                    .select(
                        "AgentHouse 是面向 human + AI agent 协作的工作区桌面环境。",
                        "AgentHouse is a workspace-first desktop environment for human + AI agent collaboration.",
                    )
                    .to_string()
            })
    };

    div()
        .size_full()
        .min_w_0()
        .min_h_0()
        .bg(theme.terminal_bg)
        .map(scroll_y)
        .flex()
        .justify_center()
        .font_family(UI_FONT_SANS)
        .child(
            div()
                .w(relative(1.0))
                .max_w(px(GLASS_BROWSER_PAGE_MAX_W_PX))
                .px(px(GLASS_BROWSER_PAGE_PADDING_X_PX))
                .py(px(GLASS_BROWSER_PAGE_PADDING_Y_PX))
                .child(
                    div()
                        .font_family(UI_FONT_SANS)
                        .text_size(px(GLASS_BROWSER_PAGE_TITLE_SIZE_PX))
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme.text)
                        .mb(px(GLASS_BROWSER_PAGE_TITLE_MARGIN_B_PX))
                        .line_clamp(1)
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .font_family(UI_FONT_SANS)
                        .text_size(px(GLASS_BROWSER_PAGE_SUBTITLE_SIZE_PX))
                        .line_height(px(GLASS_BROWSER_PAGE_SUBTITLE_LINE_HEIGHT_PX))
                        .text_color(if has_error {
                            theme.error
                        } else {
                            theme.text_muted
                        })
                        .mb(px(GLASS_BROWSER_PAGE_SUBTITLE_MARGIN_B_PX))
                        .child(subtitle),
                )
                .child(browser_preview_cards(preview, has_error, theme, language)),
        )
}

fn browser_preview_cards(
    preview: &WebPreviewSnapshot,
    has_error: bool,
    theme: AgentHouseTheme,
    language: UiLanguage,
) -> Div {
    let status = if has_error {
        language.select("失败", "Failed").to_string()
    } else {
        browser_preview_status_label(preview, language)
    };
    div()
        .grid()
        .grid_cols(2)
        .gap(px(GLASS_BROWSER_CARD_GAP_PX))
        .child(browser_preview_card(
            AppIcon::Web,
            language.select("浏览状态", "Browser"),
            status,
            theme,
        ))
        .child(browser_preview_card(
            AppIcon::Code,
            language.select("当前地址", "Address"),
            preview.url.clone(),
            theme,
        ))
        .child(browser_preview_card(
            AppIcon::Folder,
            language.select("内容", "Content"),
            preview
                .byte_count
                .map(|bytes| format!("{bytes} bytes"))
                .unwrap_or_else(|| {
                    language
                        .select("等待页面内容", "Waiting for page content")
                        .to_string()
                }),
            theme,
        ))
        .child(browser_preview_card(
            AppIcon::SplitHorizontal,
            language.select("协作表面", "Split Panes"),
            language
                .select(
                    "终端、浏览器和文件可以在同一个工作区并排工作。",
                    "Terminal, browser, and files can work side by side in one workspace.",
                )
                .to_string(),
            theme,
        ))
}

fn browser_preview_card(
    icon: AppIcon,
    title: &'static str,
    body: String,
    theme: AgentHouseTheme,
) -> Div {
    div()
        .rounded(px(GLASS_BROWSER_CARD_RADIUS_PX))
        .border_1()
        .border_color(theme.border)
        .bg(theme.panel_alt_bg)
        .p(px(GLASS_BROWSER_CARD_PADDING_PX))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(GLASS_BROWSER_CARD_TITLE_GAP_PX))
                .mb(px(GLASS_BROWSER_CARD_TITLE_MARGIN_B_PX))
                .font_family(UI_FONT_SANS)
                .text_size(px(GLASS_BROWSER_CARD_TITLE_SIZE_PX))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.text)
                .child(app_icon(icon, theme.text_muted, GLASS_BROWSER_CARD_ICON_PX))
                .child(title),
        )
        .child(
            div()
                .font_family(UI_FONT_SANS)
                .text_size(px(GLASS_BROWSER_CARD_BODY_SIZE_PX))
                .line_height(px(GLASS_BROWSER_CARD_BODY_LINE_HEIGHT_PX))
                .text_color(theme.text_muted)
                .line_clamp(2)
                .child(body),
        )
}

impl AgentHouseShell {
    fn browser_content_shell(
        &self,
        session_id: SessionId,
        focus_handle: FocusHandle,
        theme: AgentHouseTheme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let click_focus_handle = focus_handle.clone();
        let shell = cx.weak_entity();
        div()
            .on_children_prepainted(move |bounds, _window, cx| {
                let Some(bounds) = bounds.first().cloned() else {
                    return;
                };
                let _ = shell.update(cx, |this, _cx| {
                    let previous = this.browser_content_bounds.insert(session_id, bounds);
                    if previous.is_none_or(|previous| browser_bounds_changed(previous, bounds)) {
                        let viewport = viewport_for_bounds(bounds);
                        match this.browser_by_session_mut(session_id) {
                            Some(browser) => {
                                if let Err(error) = browser.resize(viewport) {
                                    this.push_event(
                                        "warn",
                                        "web",
                                        format!(
                                            "failed to resize browser session {session_id:?} from content bounds: {error}"
                                        ),
                                    );
                                }
                            }
                            None => {}
                        }
                    }
                });
            })
            .id(format!("browser-content-{session_id:?}"))
            .track_focus(&focus_handle)
            .cursor_pointer()
            .flex_1()
            .min_h_0()
            .rounded_sm()
            .border_1()
            .border_color(theme.border)
            .bg(theme.panel_bg)
            .overflow_hidden()
            .on_click(cx.listener(move |this, event, window, cx| {
                window.focus(&click_focus_handle, cx);
                if let gpui::ClickEvent::Mouse(mouse) = event {
                    if let Some((x, y)) =
                        this.browser_local_point(session_id, mouse.up.position)
                    {
                        this.send_browser_input_from_ui(
                            session_id,
                            BrowserInput::MouseClick { x, y },
                            cx,
                        );
                    }
                }
            }))
            .on_scroll_wheel(
                cx.listener(move |this, event: &ScrollWheelEvent, _window, cx| {
                    let Some((x, y)) = this.browser_local_point(session_id, event.position) else {
                        return;
                    };
                    let delta = event.delta.pixel_delta(px(16.0));
                    this.send_browser_input_from_ui(
                        session_id,
                        BrowserInput::MouseScroll {
                            x,
                            y,
                            delta_x: delta.x.as_f32().round() as i32,
                            delta_y: delta.y.as_f32().round() as i32,
                        },
                        cx,
                    );
                    cx.stop_propagation();
                }),
            )
            .on_key_down(cx.listener(move |this, event, _window, cx| {
                this.handle_browser_content_key(session_id, event, cx);
            }))
    }

    fn web_content_body(
        &self,
        session_id: SessionId,
        preview: &WebPreviewSnapshot,
        frame: Option<&BrowserFrameSnapshot>,
        focus_handle: FocusHandle,
        theme: AgentHouseTheme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let shell = self.browser_content_shell(session_id, focus_handle, theme, cx);
        if let Some(frame) = frame {
            return match frame.format {
                BrowserFrameFormat::Png => shell
                    .child(
                        div().size_full().min_w_0().min_h_0().child(
                            img(Arc::new(Image::from_bytes(
                                ImageFormat::Png,
                                frame.bytes.clone(),
                            )))
                            .size_full(),
                        ),
                    )
                    .into_any_element(),
                BrowserFrameFormat::Rgba8 => shell
                    .child(
                        div()
                            .size_full()
                            .min_w_0()
                            .min_h_0()
                            .px_2()
                            .py_2()
                            .font_family(UI_FONT_MONO)
                            .text_size(px(11.0))
                            .text_color(theme.text_muted)
                            .child(format!(
                                "browser frame ready: {}x{} {:?}, {} bytes",
                                frame.width, frame.height, frame.format, frame.byte_count
                            )),
                    )
                    .into_any_element(),
            };
        }

        let has_error = preview.error.is_some();

        shell
            .child(browser_preview_page(
                preview,
                has_error,
                theme,
                self.ui_language,
            ))
            .into_any_element()
    }
}

fn pane_icon_button(
    id: impl Into<ElementId>,
    icon: AppIcon,
    tooltip: Option<&'static str>,
    theme: AgentHouseTheme,
) -> Stateful<Div> {
    let id = id.into();
    let group = SharedString::from(format!("pane-icon-button-{id}"));
    let button = div()
        .id(id)
        .group(group.clone())
        .cursor_pointer()
        .rounded(px(GLASS_PANE_ACTION_RADIUS_PX))
        .flex()
        .items_center()
        .justify_center()
        .size(px(PANE_ACTION_SIZE_PX))
        .p_0()
        .flex_shrink_0()
        .hover(move |style| style.bg(theme.hover_bg).text_color(theme.text))
        .child(hoverable_app_icon(
            icon,
            theme.text_subtle,
            theme.text,
            WINDOW_TAB_ICON_SIZE_PX,
            group,
        ));
    if let Some(tooltip) = tooltip {
        button.tooltip(glass_tooltip(tooltip, theme))
    } else {
        button
    }
}

#[derive(Clone, Copy)]
enum AppIcon {
    Code,
    Folder,
    FolderOpen,
    SplitHorizontal,
    SplitVertical,
    Web,
}

fn hoverable_app_icon(
    icon: AppIcon,
    base_color: Rgba,
    _hover_color: Rgba,
    icon_size_px: f32,
    _group: SharedString,
) -> impl IntoElement {
    app_icon(icon, base_color, icon_size_px)
}

fn app_icon(icon: AppIcon, color: Rgba, icon_size_px: f32) -> impl IntoElement {
    let color = rgba_to_hex(color);
    let body = match icon {
        AppIcon::Code => format!(r#"<path d="{CODE_ICON_PATH}" fill="{color}"/>"#),
        AppIcon::Folder => FOLDER_ICON_BODY.replace("__COLOR__", color.as_str()),
        AppIcon::FolderOpen => FOLDER_OPEN_ICON_BODY.replace("__COLOR__", color.as_str()),
        AppIcon::SplitHorizontal => {
            format!(r#"<path d="{SPLIT_RIGHT_ICON_PATH}" fill="{color}"/>"#)
        }
        AppIcon::SplitVertical => format!(r#"<path d="{SPLIT_DOWN_ICON_PATH}" fill="{color}"/>"#),
        AppIcon::Web => format!(r#"<path d="{WEB_ICON_PATH}" fill="{color}"/>"#),
    };
    let svg = format!(
        r##"<svg viewBox="0 0 1024 1024" xmlns="http://www.w3.org/2000/svg">{body}</svg>"##
    );
    img(Arc::new(Image::from_bytes(
        ImageFormat::Svg,
        svg.into_bytes(),
    )))
    .size(px(icon_size_px))
    .flex_shrink_0()
}

fn transparent_rgba() -> Rgba {
    rgba(0x00000000)
}

fn rgba_to_hex(color: Rgba) -> String {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        channel(color.r),
        channel(color.g),
        channel(color.b)
    )
}

const WEB_ICON_PATH: &str = "M698.026667 597.333333C701.44 569.173333 704 541.013333 704 512 704 482.986667 701.44 454.826667 698.026667 426.666667L842.24 426.666667C849.066667 453.973333 853.333333 482.56 853.333333 512 853.333333 541.44 849.066667 570.026667 842.24 597.333333M622.506667 834.56C648.106667 787.2 667.733333 736 681.386667 682.666667L807.253333 682.666667C766.293333 753.066667 701.013333 807.68 622.506667 834.56M611.84 597.333333 412.16 597.333333C407.893333 569.173333 405.333333 541.013333 405.333333 512 405.333333 482.986667 407.893333 454.4 412.16 426.666667L611.84 426.666667C615.68 454.4 618.666667 482.986667 618.666667 512 618.666667 541.013333 615.68 569.173333 611.84 597.333333M512 851.626667C476.586667 800.426667 448 743.68 430.506667 682.666667L593.493333 682.666667C576 743.68 547.413333 800.426667 512 851.626667M341.333333 341.333333 216.746667 341.333333C257.28 270.506667 322.986667 215.893333 401.066667 189.44 375.466667 236.8 356.266667 288 341.333333 341.333333M216.746667 682.666667 341.333333 682.666667C356.266667 736 375.466667 787.2 401.066667 834.56 322.986667 807.68 257.28 753.066667 216.746667 682.666667M181.76 597.333333C174.933333 570.026667 170.666667 541.44 170.666667 512 170.666667 482.56 174.933333 453.973333 181.76 426.666667L325.973333 426.666667C322.56 454.826667 320 482.986667 320 512 320 541.013333 322.56 569.173333 325.973333 597.333333M512 171.946667C547.413333 223.146667 576 280.32 593.493333 341.333333L430.506667 341.333333C448 280.32 476.586667 223.146667 512 171.946667M807.253333 341.333333 681.386667 341.333333C667.733333 288 648.106667 236.8 622.506667 189.44 701.013333 216.32 766.293333 270.506667 807.253333 341.333333M512 85.333333C276.053333 85.333333 85.333333 277.333333 85.333333 512 85.333333 747.52 276.48 938.666667 512 938.666667 747.52 938.666667 938.666667 747.52 938.666667 512 938.666667 276.48 747.52 85.333333 512 85.333333Z";
const CODE_ICON_PATH: &str = "M516.032 673.024c0 4.352 3.392 8 7.488 8h184.96c4.096 0 7.488-3.648 7.488-8v-48c0-4.48-3.392-8-7.488-8H523.52c-4.096 0-7.488 3.584-7.488 8v48z m-194.944 6.08l192-161.024a8.064 8.064 0 0 0 0-12.288l-192-160.896a7.936 7.936 0 0 0-13.12 6.08v62.72a7.68 7.68 0 0 0 2.944 6.08L420.672 512 310.912 604.16a8.064 8.064 0 0 0-2.88 6.144v62.72c0 6.784 7.872 10.496 13.056 6.08zM880 112H144a32 32 0 0 0-32 32v736a32 32 0 0 0 32 32h736a32 32 0 0 0 32-32V144a32 32 0 0 0-32-32z m-40 728H184V184h656v656z";
const SPLIT_RIGHT_ICON_PATH: &str = "M161.05472 919.3472h701.9008a58.71616 58.71616 0 0 0 58.65472-58.65472V180.40832a58.71616 58.71616 0 0 0-58.65472-58.65472H161.09568a58.03008 58.03008 0 0 0-41.4208 17.08032A58.1632 58.1632 0 0 0 102.4 180.30592v680.38656a58.64448 58.64448 0 0 0 58.65472 58.65472z m385.15712-589.568V190.08512h306.95424v660.93056H546.21184V329.7792zM170.83392 190.08512h306.95424v660.93056H170.83392V190.08512z";
const SPLIT_DOWN_ICON_PATH: &str = "M171.85792 110.9504a58.65472 58.65472 0 0 0-58.65472 58.65472v701.9008a58.7264 58.7264 0 0 0 58.65472 58.65472h680.28416a58.7264 58.7264 0 0 0 58.65472-58.65472V169.64608a57.98912 57.98912 0 0 0-17.08032-41.41056 58.1632 58.1632 0 0 0-41.472-17.27488H171.85792z m670.60736 750.77632H181.53472V554.77248h660.93056v306.95424z m0-375.38816H181.53472V179.38432h660.93056v306.95424z";
const FOLDER_ICON_BODY: &str = r#"<path d="M860.16 869.3248H163.84a84.5312 84.5312 0 0 1-84.48-84.4288V239.104A84.5312 84.5312 0 0 1 163.84 154.6752h300.5952a120.6272 120.6272 0 0 1 94.8736 46.592l46.8992 60.672a65.3824 65.3824 0 0 0 51.2 25.2416H860.16a84.5312 84.5312 0 0 1 84.48 84.4288v413.2864a84.5312 84.5312 0 0 1-84.48 84.4288zM163.84 200.7552a38.4 38.4 0 0 0-38.4 38.3488v545.792a38.4 38.4 0 0 0 38.4 38.3488h696.32a38.4 38.4 0 0 0 38.3488-38.3488V371.6096a38.4 38.4 0 0 0-38.3488-38.3488h-202.5472a111.7184 111.7184 0 0 1-87.8592-43.1616l-46.8992-60.672a74.2912 74.2912 0 0 0-58.4192-28.672z" fill="__COLOR__"/><path d="M819.2 429.6192H114.432a23.04 23.04 0 1 1 0-46.08H819.2a23.04 23.04 0 0 1 0 46.08z" fill="__COLOR__"/>"#;
const FOLDER_OPEN_ICON_BODY: &str = r#"<path d="M810.666667 426.666667h88.064c67.904 0 103.082667 60.202667 70.037333 119.317333l-152.213333 272.32a136.746667 136.746667 0 0 1-24.533334 31.36A106.346667 106.346667 0 0 1 704.085333 896H125.269333c-56.256 0-90.069333-41.344-81.194666-89.173333A107.712 107.712 0 0 1 42.666667 789.44V234.581333A106.666667 106.666667 0 0 1 149.354667 128H358.4a42.666667 42.666667 0 0 1 35.498667 18.986667l55.594666 83.413333h254.293334A106.858667 106.858667 0 0 1 810.666667 337.109333V426.666667z m-85.333334 0v-89.557334c0-11.733333-9.664-21.376-21.546666-21.376H426.666667a42.666667 42.666667 0 0 1-35.498667-18.986666L335.573333 213.333333H149.333333A21.333333 21.333333 0 0 0 128 234.581333v411.904l79.445333-142.122666C232.234667 460.010667 289.088 426.666667 339.904 426.666667H725.333333z m164.693334 85.333333H339.882667c-19.904 0-48.277333 16.64-57.962667 33.984L133.973333 810.666667h550.101334c19.904 0 48.277333-16.64 57.962666-33.984L890.026667 512z" fill="__COLOR__"/>"#;

fn empty_pane_body(theme: AgentHouseTheme) -> impl IntoElement {
    div().flex_1().min_w_0().min_h_0().bg(theme.panel_bg)
}

fn workspace_initials(name: &str) -> String {
    let mut initials = name
        .split(|ch: char| ch.is_whitespace() || ch == '-' || ch == '_' || ch == '.')
        .filter_map(|part| part.chars().find(|ch| ch.is_alphanumeric()))
        .take(2)
        .collect::<String>();
    if initials.is_empty() {
        initials = "AH".to_string();
    }
    initials.to_uppercase()
}

fn workspace_root_label(path: &Path) -> String {
    let display = path.display().to_string();
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return display;
    };
    match path.strip_prefix(home) {
        Ok(relative) if relative.as_os_str().is_empty() => "~".to_string(),
        Ok(relative) => format!("~/{}", relative.display()),
        Err(_) => display,
    }
}

fn workspace_avatar_colors(index: usize, theme: AgentHouseTheme) -> (Rgba, Rgba) {
    match index % 4 {
        0 => (theme.tag_green_bg, theme.tag_green_text),
        1 => (theme.tag_blue_bg, theme.tag_blue_text),
        2 => (theme.tag_amber_bg, theme.tag_amber_text),
        _ => (theme.tag_red_bg, theme.tag_red_text),
    }
}

fn workspace_meta_label(windows: usize, tabs: usize, _language: UiLanguage) -> String {
    format!("{windows} windows · {tabs} tabs")
}

fn pane_window_frame(active: bool, theme: AgentHouseTheme) -> (Rgba, f32) {
    if active {
        (theme.active_border, GLASS_PANE_ACTIVE_SHADOW_WIDTH_PX)
    } else {
        (theme.pane_frame_border, GLASS_PANE_SHADOW_WIDTH_PX)
    }
}

fn tab_content_icon(content: &WindowContent) -> AppIcon {
    match content {
        WindowContent::Terminal { .. } => AppIcon::Code,
        WindowContent::Web { .. } => AppIcon::Web,
        WindowContent::FilePreview { .. } => AppIcon::Folder,
    }
}

fn pane_state_body(message: &'static str, theme: AgentHouseTheme, error: bool) -> Div {
    div()
        .flex()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .items_center()
        .justify_center()
        .bg(if error {
            theme.error_bg
        } else {
            theme.panel_bg
        })
        .font_family(UI_FONT_SANS)
        .text_size(px(12.0))
        .text_color(if error { theme.error } else { theme.text_muted })
        .child(message)
}

fn terminal_surface_title(terminal: &TerminalRuntime) -> String {
    terminal.view.session.name.clone()
}

fn terminal_surface_badge_label(terminal: &TerminalRuntime, language: UiLanguage) -> &'static str {
    terminal_surface_badge_label_for_runtime(
        terminal.active_command.is_some(),
        &terminal.view.session.status,
        language,
    )
}

fn terminal_surface_badge_label_for_runtime(
    active_command_running: bool,
    status: &ah_session::SessionStatus,
    language: UiLanguage,
) -> &'static str {
    if active_command_running {
        return language.select("活动中", "Active");
    }
    terminal_surface_badge_label_for_status(status, language)
}

fn terminal_surface_badge_label_for_status(
    status: &ah_session::SessionStatus,
    language: UiLanguage,
) -> &'static str {
    match status {
        ah_session::SessionStatus::Starting => language.select("启动中", "Starting"),
        ah_session::SessionStatus::Running => "",
        ah_session::SessionStatus::Blocked { .. } => language.select("阻塞", "Blocked"),
        ah_session::SessionStatus::Exited { .. } => language.select("已退出", "Exited"),
    }
}

fn terminal_surface_header(
    title: impl Into<SharedString>,
    status: impl Into<SharedString>,
    theme: AgentHouseTheme,
) -> Div {
    let status = status.into();
    let mut header = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(GLASS_TERMINAL_HEADER_GAP_PX))
        .px(px(GLASS_TERMINAL_HEADER_PADDING_X_PX))
        .py(px(GLASS_TERMINAL_HEADER_PADDING_Y_PX))
        .border_b_1()
        .border_color(theme.border_term)
        .bg(theme.terminal_panel_bg)
        .child(
            div()
                .min_w_0()
                .flex_1()
                .font_family(UI_FONT_MONO)
                .text_size(px(GLASS_TERMINAL_HEADER_TITLE_SIZE_PX))
                .text_color(theme.terminal_placeholder)
                .line_clamp(1)
                .child(title.into()),
        );
    if !status.is_empty() {
        header = header.child(
            div()
                .flex_shrink_0()
                .rounded_full()
                .px(px(GLASS_HEADER_BADGE_PADDING_X_PX))
                .py(px(GLASS_HEADER_BADGE_PADDING_Y_PX))
                .bg(theme.tag_green_bg)
                .font_family(UI_FONT_MONO)
                .text_size(px(GLASS_HEADER_BADGE_TEXT_SIZE_PX))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.tag_green_text)
                .line_clamp(1)
                .child(status),
        );
    }
    header
}

fn pane_grid_columns(pane_count: usize, mode: &LayoutMode) -> u16 {
    let count = pane_count.clamp(1, MAX_WORKSPACE_PANES);
    match mode {
        LayoutMode::Single if count <= 1 => 1,
        LayoutMode::Grid => square_grid_columns(count),
        LayoutMode::Single | LayoutMode::Columns => match count {
            1 => 1,
            2 => 2,
            3 | 4 => 2,
            5..=9 => 3,
            _ => 4,
        },
    }
}

fn square_grid_columns(pane_count: usize) -> u16 {
    match pane_count {
        1 | 2 => 1,
        3 | 4 => 2,
        5..=9 => 3,
        _ => 4,
    }
}

fn pane_grid_rows(pane_count: usize, cols: u16) -> u16 {
    let count = pane_count.clamp(1, MAX_WORKSPACE_PANES);
    let cols = usize::from(cols.max(1));
    count.div_ceil(cols) as u16
}

fn terminal_session_id_for_tab(tab: &WindowTab) -> Option<SessionId> {
    match &tab.content {
        WindowContent::Terminal { session_id } => Some(*session_id),
        WindowContent::Web { .. } | WindowContent::FilePreview { .. } => None,
    }
}

fn browser_session_id_for_tab(tab: &WindowTab) -> Option<SessionId> {
    match &tab.content {
        WindowContent::Web { session_id, .. } => Some(*session_id),
        WindowContent::Terminal { .. } | WindowContent::FilePreview { .. } => None,
    }
}

fn terminal_session_ids_for_window(window: &WorkspaceWindow) -> Vec<SessionId> {
    window
        .tabs
        .iter()
        .filter_map(terminal_session_id_for_tab)
        .collect()
}

fn browser_session_ids_for_window(window: &WorkspaceWindow) -> Vec<SessionId> {
    window
        .tabs
        .iter()
        .filter_map(browser_session_id_for_tab)
        .collect()
}

fn terminal_session_ids_for_workspace(workspace: &Workspace) -> Vec<SessionId> {
    workspace
        .windows
        .iter()
        .flat_map(terminal_session_ids_for_window)
        .collect()
}

fn browser_session_ids_for_workspace(workspace: &Workspace) -> Vec<SessionId> {
    workspace
        .windows
        .iter()
        .flat_map(browser_session_ids_for_window)
        .collect()
}

fn terminal_session_ids_for_workspaces(workspaces: &[Workspace]) -> HashSet<SessionId> {
    workspaces
        .iter()
        .flat_map(terminal_session_ids_for_workspace)
        .collect()
}

fn terminal_key_from_control(key: TerminalKeyInput) -> TerminalKey {
    TerminalKey {
        key: key.key,
        text: key.text,
        modifiers: TerminalKeyModifiers {
            alt: key.modifiers.alt,
            control: key.modifiers.control,
            shift: key.modifiers.shift,
            platform: key.modifiers.platform,
        },
    }
}

fn split_direction_from_control(direction: WindowSplitDirection) -> SplitDirection {
    match direction {
        WindowSplitDirection::Right => SplitDirection::Right,
        WindowSplitDirection::Down => SplitDirection::Down,
    }
}

fn block_kind_label(kind: &BlockKind) -> &'static str {
    match kind {
        BlockKind::Command => "Command",
        BlockKind::AgentInput => "AgentInput",
        BlockKind::AgentOutput => "AgentOutput",
        BlockKind::FileRef => "FileRef",
        BlockKind::WebRef => "WebRef",
        BlockKind::System => "System",
    }
}

#[allow(dead_code)]
fn block_state_label(state: &BlockState) -> SharedString {
    match state {
        BlockState::Streaming => "streaming".into(),
        BlockState::Complete => "complete".into(),
        BlockState::Collapsed => "collapsed".into(),
        BlockState::Pinned => "pinned".into(),
        BlockState::Forwarded { .. } => "forwarded".into(),
    }
}

#[allow(dead_code)]
fn block_text_for_display(text: &str) -> String {
    if text.trim().is_empty() {
        "waiting for terminal output".to_string()
    } else {
        wrap_long_lines(
            &tail_chars(text, MAX_BLOCK_DISPLAY_CHARS),
            MAX_DISPLAY_LINE_CHARS,
        )
    }
}

fn file_preview_snapshot(path: &Path) -> FilePreviewSnapshot {
    let path_buf = path.to_path_buf();
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => {
            return FilePreviewSnapshot {
                path: path_buf,
                kind: "missing".to_string(),
                status: format!("failed to read metadata: {error}"),
                text: None,
                byte_count: None,
                truncated: false,
            };
        }
    };

    if metadata.is_dir() {
        return directory_preview_snapshot(path);
    }

    let byte_count = metadata.len();
    let mut file = match fs::File::open(path) {
        Ok(file) => file.take((MAX_FILE_PREVIEW_BYTES + 1) as u64),
        Err(error) => {
            return FilePreviewSnapshot {
                path: path_buf,
                kind: "file".to_string(),
                status: format!("failed to open file: {error}"),
                text: None,
                byte_count: Some(byte_count),
                truncated: false,
            };
        }
    };
    let mut bytes = Vec::new();
    if let Err(error) = file.read_to_end(&mut bytes) {
        return FilePreviewSnapshot {
            path: path_buf,
            kind: "file".to_string(),
            status: format!("failed to read file: {error}"),
            text: None,
            byte_count: Some(byte_count),
            truncated: false,
        };
    }

    let truncated = bytes.len() > MAX_FILE_PREVIEW_BYTES;
    if truncated {
        bytes.truncate(MAX_FILE_PREVIEW_BYTES);
    }
    let text = String::from_utf8_lossy(&bytes).to_string();
    FilePreviewSnapshot {
        path: path_buf,
        kind: "file".to_string(),
        status: if truncated {
            format!("file: {byte_count} bytes, preview truncated")
        } else {
            format!("file: {byte_count} bytes")
        },
        text: Some(text),
        byte_count: Some(byte_count),
        truncated,
    }
}

fn directory_preview_snapshot(path: &Path) -> FilePreviewSnapshot {
    let mut entries = Vec::new();
    let mut truncated = false;
    match fs::read_dir(path) {
        Ok(read_dir) => {
            for entry in read_dir {
                if entries.len() >= MAX_DIRECTORY_PREVIEW_ENTRIES {
                    truncated = true;
                    break;
                }
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(error) => {
                        entries.push(format!("<read error: {error}>"));
                        continue;
                    }
                };
                let is_dir = entry.file_type().is_ok_and(|file_type| file_type.is_dir());
                let suffix = if is_dir { "/" } else { "" };
                entries.push(format!("{}{suffix}", entry.file_name().to_string_lossy()));
            }
        }
        Err(error) => {
            return FilePreviewSnapshot {
                path: path.to_path_buf(),
                kind: "directory".to_string(),
                status: format!("failed to read directory: {error}"),
                text: None,
                byte_count: None,
                truncated: false,
            };
        }
    }

    entries.sort();
    FilePreviewSnapshot {
        path: path.to_path_buf(),
        kind: "directory".to_string(),
        status: if truncated {
            format!("directory: showing first {MAX_DIRECTORY_PREVIEW_ENTRIES} entries")
        } else {
            format!("directory: {} entries", entries.len())
        },
        text: Some(entries.join("\n")),
        byte_count: None,
        truncated,
    }
}

fn finalize_command_block_text(block: &mut Block, command: &str) {
    if !looks_like_claude_output(command, &block.text) {
        return;
    }
    let raw = block.text.clone();
    let Some(cleaned) = clean_claude_json_text(&raw) else {
        return;
    };
    if cleaned == raw.trim() {
        return;
    }

    if let Some(path) = write_raw_block_output(&raw) {
        block.attach(BlockAttachment::File { path });
    }
    block.text = cleaned;
    if !block.text.ends_with('\n') {
        block.text.push('\n');
    }
}

fn clean_forwarded_block_text(text: &str) -> String {
    clean_claude_json_text(text).unwrap_or_else(|| text.to_string())
}

fn looks_like_claude_output(command: &str, text: &str) -> bool {
    command
        .split_whitespace()
        .any(|part| part == "claude" || part.ends_with("/claude"))
        || text.contains(r#""type":"stream_event""#)
        || text.contains(r#""type":"assistant""#)
        || text.contains(r#""type":"result""#)
}

fn clean_claude_json_text(text: &str) -> Option<String> {
    let mut assistant_text = String::new();
    let mut stream_text = String::new();
    for value in parse_json_objects(text) {
        if let Some(result) = value.get("result").and_then(serde_json::Value::as_str)
            && !result.trim().is_empty()
        {
            return Some(normalize_model_text(result));
        }
        collect_assistant_message_text(&value, &mut assistant_text);
        collect_stream_delta_text(&value, &mut stream_text);
    }

    if !assistant_text.trim().is_empty() {
        Some(normalize_model_text(&assistant_text))
    } else if !stream_text.trim().is_empty() {
        Some(normalize_model_text(&stream_text))
    } else {
        None
    }
}

fn collect_assistant_message_text(value: &serde_json::Value, output: &mut String) {
    if value.get("type").and_then(serde_json::Value::as_str) != Some("assistant") {
        return;
    }
    let Some(content) = value
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(serde_json::Value::as_array)
    else {
        return;
    };
    for item in content {
        if item.get("type").and_then(serde_json::Value::as_str) == Some("text")
            && let Some(text) = item.get("text").and_then(serde_json::Value::as_str)
        {
            output.push_str(text);
        }
    }
}

fn collect_stream_delta_text(value: &serde_json::Value, output: &mut String) {
    if value.get("type").and_then(serde_json::Value::as_str) != Some("stream_event") {
        return;
    }
    let Some(delta) = value
        .get("event")
        .and_then(|event| event.get("delta"))
        .and_then(|delta| delta.get("text"))
        .and_then(serde_json::Value::as_str)
    else {
        return;
    };
    output.push_str(delta);
}

fn normalize_model_text(text: &str) -> String {
    let mut cleaned = String::new();
    for line in text.lines() {
        let line = line.trim_end();
        if !cleaned.is_empty() {
            cleaned.push('\n');
        }
        cleaned.push_str(line);
    }
    cleaned.trim().to_string()
}

fn write_raw_block_output(text: &str) -> Option<PathBuf> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis();
    let path = std::env::temp_dir().join(format!("agenthouse-raw-block-{timestamp}.jsonl"));
    fs::write(&path, text).ok()?;
    Some(path)
}

fn parse_json_objects(text: &str) -> Vec<serde_json::Value> {
    let mut values = Vec::new();
    let mut object = String::new();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escape = false;
    let mut collecting = false;

    for ch in text.chars() {
        if !collecting {
            if ch == '{' {
                collecting = true;
                depth = 1;
                object.clear();
                object.push(ch);
            }
            continue;
        }

        if in_string {
            if escape {
                object.push(ch);
                escape = false;
            } else if ch == '\\' {
                object.push(ch);
                escape = true;
            } else if ch == '"' {
                object.push(ch);
                in_string = false;
            } else if ch != '\n' && ch != '\r' {
                object.push(ch);
            }
            continue;
        }

        match ch {
            '"' => {
                object.push(ch);
                in_string = true;
            }
            '{' => {
                object.push(ch);
                depth = depth.saturating_add(1);
            }
            '}' => {
                object.push(ch);
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&object) {
                        values.push(value);
                    }
                    collecting = false;
                    object.clear();
                }
            }
            _ => object.push(ch),
        }
    }

    values
}

#[cfg(test)]
fn clean_workspace_for_ui(existing_workspace_count: usize, cwd: PathBuf) -> Workspace {
    workspace_for_root(existing_workspace_count, cwd)
}

fn workspace_for_root(existing_workspace_count: usize, root: PathBuf) -> Workspace {
    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("Workspace {}", existing_workspace_count + 1));

    Workspace::new(name, Some(root))
}

fn canonical_workspace_root(root: PathBuf) -> PathBuf {
    root.canonicalize().unwrap_or(root)
}

fn workspace_window_for_ui(existing_window_count: usize) -> WorkspaceWindow {
    WorkspaceWindow::new(format!("Window {}", existing_window_count + 1))
}

fn scroll_x<T: Styled>(mut element: T) -> T {
    element.style().overflow.x = Some(Overflow::Scroll);
    element
}

fn scroll_y<T: Styled>(mut element: T) -> T {
    element.style().overflow.y = Some(Overflow::Scroll);
    element
}

fn viewport_for_bounds(bounds: Bounds<Pixels>) -> ViewportSize {
    ViewportSize {
        width: bounds.size.width.as_f32().round().max(1.0) as u32,
        height: bounds.size.height.as_f32().round().max(1.0) as u32,
    }
}

fn browser_bounds_changed(previous: Bounds<Pixels>, next: Bounds<Pixels>) -> bool {
    viewport_for_bounds(previous) != viewport_for_bounds(next)
}

#[allow(dead_code)]
fn terminal_input_for_display(input: &str) -> String {
    input
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

#[allow(dead_code)]
fn terminal_prompt_submission(
    input: &str,
    command_running: bool,
) -> Option<TerminalPromptSubmission> {
    if command_running {
        let mut stdin = input.to_string();
        stdin.push('\n');
        return Some(TerminalPromptSubmission::Stdin(stdin));
    }

    let command = input.trim();
    if command.is_empty() {
        None
    } else {
        Some(TerminalPromptSubmission::Command(command.to_string()))
    }
}

fn normalize_browser_address(address: &str) -> String {
    let address = address.trim();
    if address.eq_ignore_ascii_case("about:blank")
        || address.contains("://")
        || address.starts_with("about:")
    {
        return address.to_string();
    }

    if address.starts_with("localhost")
        || address.starts_with("127.")
        || address.starts_with("0.0.0.0")
    {
        return format!("http://{address}");
    }

    format!("https://{address}")
}

fn sanitize_browser_address_insert_text(text: &str) -> String {
    text.chars()
        .filter(|ch| !matches!(ch, '\n' | '\r' | '\t'))
        .collect()
}

fn clamp_to_char_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn clamp_range_to_text(text: &str, range: Range<usize>) -> Range<usize> {
    let start = clamp_to_char_boundary(text, range.start);
    let end = clamp_to_char_boundary(text, range.end);
    if start <= end { start..end } else { end..start }
}

fn previous_text_boundary(text: &str, offset: usize) -> usize {
    let offset = clamp_to_char_boundary(text, offset);
    text.char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index < offset)
        .last()
        .unwrap_or(0)
}

fn next_text_boundary(text: &str, offset: usize) -> usize {
    let offset = clamp_to_char_boundary(text, offset);
    text.char_indices()
        .map(|(index, _)| index)
        .find(|index| *index > offset)
        .unwrap_or(text.len())
}

fn offset_from_utf16(text: &str, offset: usize) -> usize {
    let mut utf8_offset = 0;
    let mut utf16_count = 0;

    for ch in text.chars() {
        if utf16_count >= offset {
            break;
        }
        utf16_count += ch.len_utf16();
        utf8_offset += ch.len_utf8();
    }

    clamp_to_char_boundary(text, utf8_offset)
}

fn offset_to_utf16(text: &str, offset: usize) -> usize {
    let offset = clamp_to_char_boundary(text, offset);
    let mut utf16_offset = 0;
    let mut utf8_count = 0;

    for ch in text.chars() {
        if utf8_count >= offset {
            break;
        }
        utf8_count += ch.len_utf8();
        utf16_offset += ch.len_utf16();
    }

    utf16_offset
}

fn range_from_utf16(text: &str, range_utf16: &Range<usize>) -> Range<usize> {
    offset_from_utf16(text, range_utf16.start)..offset_from_utf16(text, range_utf16.end)
}

fn range_to_utf16(text: &str, range: &Range<usize>) -> Range<usize> {
    offset_to_utf16(text, range.start)..offset_to_utf16(text, range.end)
}

fn browser_address_select_to(edit: &mut BrowserAddressEditState, offset: usize) {
    if edit.selection_reversed {
        edit.selected_range.start = offset;
    } else {
        edit.selected_range.end = offset;
    }
    if edit.selected_range.end < edit.selected_range.start {
        edit.selection_reversed = !edit.selection_reversed;
        edit.selected_range = edit.selected_range.end..edit.selected_range.start;
    }
    edit.marked_range = None;
}

fn browser_address_text_runs(
    text_len: usize,
    marked_range: Option<&Range<usize>>,
    color: Hsla,
) -> Vec<TextRun> {
    let base_run = TextRun {
        len: text_len,
        font: font(UI_FONT_SANS),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let Some(marked_range) = marked_range else {
        return vec![base_run];
    };
    let marked_range = clamp_range_to_len(text_len, marked_range.clone());
    [
        TextRun {
            len: marked_range.start,
            ..base_run.clone()
        },
        TextRun {
            len: marked_range.end.saturating_sub(marked_range.start),
            underline: Some(UnderlineStyle {
                thickness: px(1.0),
                color: Some(color),
                wavy: false,
            }),
            ..base_run.clone()
        },
        TextRun {
            len: text_len.saturating_sub(marked_range.end),
            ..base_run
        },
    ]
    .into_iter()
    .filter(|run| run.len > 0)
    .collect()
}

fn clamp_range_to_len(len: usize, range: Range<usize>) -> Range<usize> {
    let start = range.start.min(len);
    let end = range.end.min(len);
    if start <= end { start..end } else { end..start }
}

fn tail_chars(text: &str, max_chars: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_chars {
        return text.to_string();
    }

    let tail: String = text.chars().skip(char_count - max_chars).collect();
    format!("[showing last {max_chars} chars]\n{tail}")
}

fn wrap_long_lines(text: &str, max_chars: usize) -> String {
    let mut output = String::new();
    for line in text.lines() {
        for wrapped in wrap_line(line, max_chars) {
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str(&wrapped);
        }
    }
    output
}

fn wrap_line(line: &str, max_chars: usize) -> Vec<String> {
    if line.chars().count() <= max_chars {
        return vec![line.to_string()];
    }

    let mut wrapped = Vec::new();
    let mut current = String::new();
    for ch in line.chars() {
        current.push(ch);
        if current.chars().count() >= max_chars {
            wrapped.push(current);
            current = String::new();
        }
    }
    if !current.is_empty() {
        wrapped.push(current);
    }
    wrapped
}

fn window_summaries(workspace: &Workspace) -> Vec<WindowSummary> {
    workspace
        .windows
        .iter()
        .map(|window| WindowSummary::from_window(workspace, window))
        .collect()
}

fn session_summary(terminal: &TerminalRuntime) -> SessionSummary {
    SessionSummary {
        id: terminal.view.session.id,
        name: terminal.view.session.name.clone(),
        status: terminal.view.status.to_string(),
        block_count: terminal.view.blocks.len(),
        ring_state: ring_state_label(&terminal.view.ring.state).to_string(),
        ring_summary: terminal.view.ring.summary.to_string(),
        unread_count: terminal.view.ring.unread_count,
    }
}

fn ring_state_label(state: &RingState) -> &'static str {
    match state {
        RingState::Idle => "idle",
        RingState::Running => "running",
        RingState::Complete => "complete",
        RingState::Error => "error",
    }
}

#[allow(dead_code)]
fn notification_ring(ring: &NotificationRing, theme: AgentHouseTheme) -> impl IntoElement {
    let color = match ring.state {
        RingState::Idle => theme.text_subtle,
        RingState::Running => theme.warning,
        RingState::Complete => theme.success,
        RingState::Error => theme.error,
    };

    div()
        .flex()
        .min_w_0()
        .max_w(px(170.0))
        .items_center()
        .gap_1()
        .child(
            div()
                .flex_shrink_0()
                .rounded_full()
                .size(px(10.0))
                .bg(color)
                .border_1()
                .border_color(theme.ring_border),
        )
        .child(
            div()
                .min_w_0()
                .text_size(px(11.0))
                .text_color(theme.text_muted)
                .line_clamp(1)
                .child(format!("{} ({})", ring.summary, ring.unread_count)),
        )
}

struct TerminalGridSizer {
    session_id: SessionId,
    shell: WeakEntity<AgentHouseShell>,
    child: Option<AnyElement>,
}

impl IntoElement for TerminalGridSizer {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TerminalGridSizer {
    type RequestLayoutState = LayoutId;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let child_layout_id = self
            .child
            .as_mut()
            .expect("terminal grid sizer child should exist")
            .request_layout(window, cx);
        let mut style = Style::default();
        style.display = Display::Flex;
        style.flex_direction = FlexDirection::Column;
        style.align_items = Some(AlignItems::Stretch);
        style.size.width = relative(1.0).into();
        style.size.height = relative(1.0).into();
        style.flex_grow = 1.0;
        style.flex_shrink = 1.0;
        style.min_size.width = px(0.0).into();
        style.min_size.height = px(0.0).into();
        style.overflow.x = Overflow::Hidden;
        style.overflow.y = Overflow::Hidden;
        let layout_id = window.request_layout(style, [child_layout_id], cx);
        (layout_id, child_layout_id)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _child_layout_id: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        if let Some(metrics) = terminal_grid_metrics_for_bounds(bounds) {
            let _ = self.shell.update(cx, |shell, cx| {
                shell.sync_terminal_grid_metrics(self.session_id, metrics, cx);
            });
        }

        if let Some(child) = self.child.as_mut() {
            child.prepaint(window, cx);
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _child_layout_id: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(child) = self.child.as_mut() {
            child.paint(window, cx);
        }

        let focus_handle = self
            .shell
            .read_with(cx, |shell, _cx| {
                shell.terminal_focus_handles.get(&self.session_id).cloned()
            })
            .ok()
            .flatten();
        if let Some(focus_handle) = focus_handle {
            window.handle_input(
                &focus_handle,
                TerminalImeInputHandler {
                    session_id: self.session_id,
                    shell: self.shell.clone(),
                    element_bounds: bounds,
                },
                cx,
            );
        }
    }
}

struct TerminalScreenElement {
    snapshot: TerminalScreenSnapshot,
    theme: AgentHouseTheme,
}

struct TerminalScreenPrepaintState {
    rows: Vec<TerminalPaintRow>,
}

struct TerminalPaintRow {
    shaped_line: ShapedLine,
    backgrounds: Vec<TerminalPaintRect>,
}

struct TerminalPaintRect {
    start_col: usize,
    cell_count: usize,
    color: Rgba,
}

impl IntoElement for TerminalScreenElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TerminalScreenElement {
    type RequestLayoutState = ();
    type PrepaintState = TerminalScreenPrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = relative(1.0).into();
        style.flex_grow = 1.0;
        style.flex_shrink = 1.0;
        style.min_size.width = px(0.0).into();
        style.min_size.height = px(0.0).into();
        style.overflow.x = Overflow::Hidden;
        style.overflow.y = Overflow::Hidden;
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
        let mut rows = Vec::with_capacity(self.snapshot.lines.len());
        let base_font = font("Menlo");

        for (row_index, line) in self.snapshot.lines.iter().enumerate() {
            let segments = terminal_line_segments(
                &line.cells,
                row_index,
                self.snapshot.cursor_row,
                self.snapshot.cursor_col,
                self.theme,
            );
            let mut text = String::new();
            let mut runs = Vec::new();
            let mut backgrounds = Vec::new();
            let mut col = 0usize;

            for segment in segments {
                let start = text.len();
                text.push_str(&segment.text);
                let len = text.len() - start;
                if len > 0 {
                    let mut run_font = base_font.clone();
                    if segment.style.bold {
                        run_font.weight = FontWeight::BOLD;
                    }
                    if segment.style.italic {
                        run_font.style = FontStyle::Italic;
                    }
                    runs.push(TextRun {
                        len,
                        font: run_font,
                        color: Hsla::from(segment.style.fg),
                        background_color: None,
                        underline: segment.style.underline.then_some(UnderlineStyle {
                            thickness: px(1.0),
                            color: Some(Hsla::from(segment.style.fg)),
                            wavy: false,
                        }),
                        strikethrough: None,
                    });
                }
                if segment.style.bg != self.theme.terminal_bg {
                    backgrounds.push(TerminalPaintRect {
                        start_col: col,
                        cell_count: segment.cell_count,
                        color: segment.style.bg,
                    });
                }
                col += segment.cell_count;
            }

            if text.is_empty() {
                text.push(' ');
                runs.push(TextRun {
                    len: 1,
                    font: base_font.clone(),
                    color: Hsla::from(self.theme.terminal_fg),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                });
            }

            let shaped_line = window.text_system().shape_line(
                SharedString::from(text),
                px(TERMINAL_FONT_SIZE_PX),
                &runs,
                None,
            );

            rows.push(TerminalPaintRow {
                shaped_line,
                backgrounds,
            });
        }

        TerminalScreenPrepaintState { rows }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.paint_quad(fill(bounds, self.theme.terminal_bg));

        for (row_index, row) in prepaint.rows.iter().enumerate() {
            let y = bounds.origin.y + px(TERMINAL_CELL_HEIGHT_PX * row_index as f32);
            for background in &row.backgrounds {
                let x = bounds.origin.x + px(TERMINAL_CELL_WIDTH_PX * background.start_col as f32);
                let width = px(TERMINAL_CELL_WIDTH_PX * background.cell_count as f32);
                window.paint_quad(fill(
                    Bounds::new(Point::new(x, y), size(width, px(TERMINAL_CELL_HEIGHT_PX))),
                    background.color,
                ));
            }

            let _ = row.shaped_line.paint(
                Point::new(bounds.origin.x, y),
                px(TERMINAL_CELL_HEIGHT_PX),
                TextAlign::Left,
                None,
                window,
                cx,
            );
        }
    }
}

struct TerminalImeInputHandler {
    session_id: SessionId,
    shell: WeakEntity<AgentHouseShell>,
    element_bounds: Bounds<Pixels>,
}

impl InputHandler for TerminalImeInputHandler {
    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<UTF16Selection> {
        None
    }

    fn marked_text_range(&mut self, _window: &mut Window, cx: &mut App) -> Option<Range<usize>> {
        self.shell
            .read_with(cx, |shell, _cx| {
                shell
                    .terminal_marked_text
                    .get(&self.session_id)
                    .map(|text| 0..text.encode_utf16().count())
            })
            .ok()
            .flatten()
    }

    fn text_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        _adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut App,
    ) -> Option<String> {
        self.shell
            .read_with(cx, |shell, _cx| {
                shell.terminal_marked_text.get(&self.session_id).cloned()
            })
            .ok()
            .flatten()
    }

    fn replace_text_in_range(
        &mut self,
        _replacement_range: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut App,
    ) {
        let _ = self.shell.update(cx, |shell, cx| {
            shell.commit_terminal_ime_text(self.session_id, text, cx);
        });
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range_utf16: Option<Range<usize>>,
        new_text: &str,
        _new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut App,
    ) {
        let _ = self.shell.update(cx, |shell, cx| {
            shell.replace_terminal_marked_text(self.session_id, new_text, cx);
        });
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut App) {
        let _ = self.shell.update(cx, |shell, cx| {
            shell.clear_terminal_marked_text(self.session_id, cx);
        });
    }

    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        _window: &mut Window,
        cx: &mut App,
    ) -> Option<Bounds<Pixels>> {
        self.shell
            .read_with(cx, |shell, _cx| {
                shell.terminal_ime_bounds_for_session(self.session_id, self.element_bounds)
            })
            .ok()
    }

    fn character_index_for_point(
        &mut self,
        _point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<usize> {
        None
    }

    fn accepts_text_input(&mut self, _window: &mut Window, _cx: &mut App) -> bool {
        true
    }

    fn prefers_ime_for_printable_keys(&mut self, _window: &mut Window, _cx: &mut App) -> bool {
        true
    }
}

struct BrowserAddressElement {
    session_id: SessionId,
    shell: WeakEntity<AgentHouseShell>,
    text: String,
    placeholder: &'static str,
    state: BrowserAddressRenderState,
    focused: bool,
    theme: AgentHouseTheme,
}

struct BrowserAddressPrepaintState {
    line: ShapedLine,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl IntoElement for BrowserAddressElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for BrowserAddressElement {
    type RequestLayoutState = ();
    type PrepaintState = BrowserAddressPrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = px(18.0).into();
        style.flex_grow = 1.0;
        style.flex_shrink = 1.0;
        style.min_size.width = px(0.0).into();
        style.overflow.x = Overflow::Hidden;
        style.overflow.y = Overflow::Hidden;
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
        let text = if self.text.is_empty() {
            SharedString::from(self.placeholder)
        } else {
            SharedString::from(self.text.clone())
        };
        let color = if self.text.is_empty() {
            Hsla::from(self.theme.text_subtle)
        } else {
            Hsla::from(self.theme.text)
        };
        let mut runs =
            browser_address_text_runs(text.len(), self.state.marked_range.as_ref(), color);
        if runs.is_empty() {
            runs.push(TextRun {
                len: text.len(),
                font: font(UI_FONT_SANS),
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            });
        }
        let line = window.text_system().shape_line(text, px(11.0), &runs, None);

        let text_len = self.text.len();
        let selected_range = clamp_range_to_text(&self.text, self.state.selected_range.clone());
        let cursor_offset = clamp_to_char_boundary(&self.text, self.state.cursor_offset);
        let cursor_x = line.x_for_index(cursor_offset.min(text_len));
        let cursor = (self.focused && selected_range.is_empty()).then(|| {
            fill(
                Bounds::new(
                    Point::new(bounds.origin.x + cursor_x, bounds.origin.y + px(2.0)),
                    size(px(1.5), bounds.size.height - px(4.0)),
                ),
                self.theme.accent,
            )
        });
        let selection = (self.focused && !selected_range.is_empty()).then(|| {
            fill(
                Bounds::from_corners(
                    Point::new(
                        bounds.origin.x + line.x_for_index(selected_range.start),
                        bounds.origin.y + px(2.0),
                    ),
                    Point::new(
                        bounds.origin.x + line.x_for_index(selected_range.end),
                        bounds.origin.y + bounds.size.height - px(2.0),
                    ),
                ),
                self.theme.active_bg,
            )
        });

        BrowserAddressPrepaintState {
            line,
            cursor,
            selection,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(focus_handle) = self
            .shell
            .read_with(cx, |shell, _cx| {
                shell
                    .browser_address_focus_handles
                    .get(&self.session_id)
                    .cloned()
            })
            .ok()
            .flatten()
        {
            window.handle_input(
                &focus_handle,
                BrowserAddressInputHandler {
                    session_id: self.session_id,
                    shell: self.shell.clone(),
                    element_bounds: bounds,
                },
                cx,
            );
        }

        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }
        let _ = prepaint.line.paint(
            bounds.origin,
            bounds.size.height,
            TextAlign::Left,
            None,
            window,
            cx,
        );
        if let Some(cursor) = prepaint.cursor.take() {
            window.paint_quad(cursor);
        }

        let line = prepaint.line.clone();
        let _ = self.shell.update(cx, |shell, _cx| {
            if let Some(edit) = shell.browser_address_edits.get_mut(&self.session_id) {
                edit.last_layout = Some(line);
                edit.last_bounds = Some(bounds);
            }
        });
    }
}

struct BrowserAddressInputHandler {
    session_id: SessionId,
    shell: WeakEntity<AgentHouseShell>,
    element_bounds: Bounds<Pixels>,
}

impl InputHandler for BrowserAddressInputHandler {
    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        cx: &mut App,
    ) -> Option<UTF16Selection> {
        self.shell
            .read_with(cx, |shell, _cx| {
                shell.browser_address_selection_utf16(self.session_id)
            })
            .ok()
    }

    fn marked_text_range(&mut self, _window: &mut Window, cx: &mut App) -> Option<Range<usize>> {
        self.shell
            .read_with(cx, |shell, _cx| {
                shell.browser_address_marked_range_utf16(self.session_id)
            })
            .ok()
            .flatten()
    }

    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut App,
    ) -> Option<String> {
        self.shell
            .read_with(cx, |shell, _cx| {
                shell.browser_address_text_for_range(self.session_id, range_utf16, adjusted_range)
            })
            .ok()
            .flatten()
    }

    fn replace_text_in_range(
        &mut self,
        replacement_range: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut App,
    ) {
        let _ = self.shell.update(cx, |shell, cx| {
            shell.replace_browser_address_text_utf16(self.session_id, replacement_range, text, cx);
        });
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut App,
    ) {
        let _ = self.shell.update(cx, |shell, cx| {
            shell.replace_and_mark_browser_address_text_utf16(
                self.session_id,
                range_utf16,
                new_text,
                new_selected_range,
                cx,
            );
        });
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut App) {
        let _ = self.shell.update(cx, |shell, cx| {
            shell.unmark_browser_address_text(self.session_id, cx);
        });
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _window: &mut Window,
        cx: &mut App,
    ) -> Option<Bounds<Pixels>> {
        self.shell
            .read_with(cx, |shell, _cx| {
                shell.browser_address_bounds_for_range(
                    self.session_id,
                    range_utf16,
                    self.element_bounds,
                )
            })
            .ok()
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        cx: &mut App,
    ) -> Option<usize> {
        self.shell
            .read_with(cx, |shell, _cx| {
                shell.browser_address_index_for_point(self.session_id, point)
            })
            .ok()
            .flatten()
    }

    fn accepts_text_input(&mut self, _window: &mut Window, _cx: &mut App) -> bool {
        true
    }

    fn prefers_ime_for_printable_keys(&mut self, _window: &mut Window, _cx: &mut App) -> bool {
        true
    }
}

fn terminal_grid_metrics_for_bounds(bounds: Bounds<Pixels>) -> Option<TerminalGridMetrics> {
    let width = f32::from(bounds.size.width) - TERMINAL_GRID_INSET_PX;
    let height = f32::from(bounds.size.height) - TERMINAL_GRID_INSET_PX;
    if width <= 0.0 || height <= 0.0 {
        return None;
    }

    let cols = (width / TERMINAL_CELL_WIDTH_PX).floor().max(1.0) as u16;
    let rows = (height / TERMINAL_CELL_HEIGHT_PX).floor().max(1.0) as u16;
    Some(TerminalGridMetrics::new(cols, rows))
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TerminalCellRenderStyle {
    fg: Rgba,
    bg: Rgba,
    bold: bool,
    italic: bool,
    underline: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct TerminalLineSegment {
    text: String,
    cell_count: usize,
    style: TerminalCellRenderStyle,
}

fn terminal_line_segments(
    cells: &[TerminalScreenCell],
    row_index: usize,
    cursor_row: usize,
    cursor_col: usize,
    theme: AgentHouseTheme,
) -> Vec<TerminalLineSegment> {
    let mut segments: Vec<TerminalLineSegment> = Vec::new();
    for (col_index, cell) in cells.iter().enumerate() {
        if cell.wide_spacer {
            continue;
        }

        let cell_count = if cell.wide { 2 } else { 1 };
        let active_cursor = cursor_row == row_index
            && (cursor_col == col_index || (cell.wide && cursor_col == col_index + 1));
        let style = terminal_cell_render_style(cell, active_cursor, theme);

        if let Some(segment) = segments.last_mut()
            && segment.style == style
        {
            segment.text.push(cell.ch);
            segment.cell_count += cell_count;
            continue;
        }

        segments.push(TerminalLineSegment {
            text: cell.ch.to_string(),
            cell_count,
            style,
        });
    }
    segments
}

fn terminal_cell_render_style(
    cell: &TerminalScreenCell,
    active_cursor: bool,
    theme: AgentHouseTheme,
) -> TerminalCellRenderStyle {
    let mut fg = terminal_color_to_rgb(cell.fg, true, theme);
    let mut bg = terminal_color_to_rgb(cell.bg, false, theme);
    if cell.inverse || active_cursor {
        std::mem::swap(&mut fg, &mut bg);
    }

    TerminalCellRenderStyle {
        fg,
        bg,
        bold: cell.bold,
        italic: cell.italic,
        underline: cell.underline,
    }
}

fn terminal_color_to_rgb(
    color: TerminalColor,
    foreground: bool,
    theme: AgentHouseTheme,
) -> gpui::Rgba {
    match color {
        TerminalColor::DefaultForeground => theme.terminal_fg,
        TerminalColor::DefaultBackground => theme.terminal_bg,
        TerminalColor::Named(index) => terminal_named_color(index, foreground, theme),
        TerminalColor::Rgb { r, g, b } => {
            gpui::rgb(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b))
        }
        TerminalColor::Indexed(index) => terminal_indexed_color(index, theme),
    }
}

fn terminal_named_color(index: u8, foreground: bool, theme: AgentHouseTheme) -> gpui::Rgba {
    match index {
        0 => rgb(0x111318),
        1 => rgb(0xff6b6b),
        2 => rgb(0x7bd88f),
        3 => rgb(0xf4d35e),
        4 => rgb(0x6ea8fe),
        5 => rgb(0xc792ea),
        6 => rgb(0x4dd4d4),
        7 => rgb(0xd6deea),
        8 => rgb(0x596171),
        9 => rgb(0xff8585),
        10 => rgb(0x9af2aa),
        11 => rgb(0xffe082),
        12 => rgb(0x8cbcff),
        13 => rgb(0xd9a7ff),
        14 => rgb(0x74f2e8),
        15 => rgb(0xffffff),
        _ if foreground => theme.terminal_fg,
        _ => theme.terminal_bg,
    }
}

fn terminal_indexed_color(index: u8, theme: AgentHouseTheme) -> gpui::Rgba {
    if index < 16 {
        return terminal_named_color(index, true, theme);
    }

    if (16..=231).contains(&index) {
        let value = index - 16;
        let r = value / 36;
        let g = (value % 36) / 6;
        let b = value % 6;
        let scale = |component: u8| {
            if component == 0 {
                0
            } else {
                55 + component * 40
            }
        };
        return gpui::rgb(
            u32::from(scale(r)) << 16 | u32::from(scale(g)) << 8 | u32::from(scale(b)),
        );
    }

    let gray = 8 + (index.saturating_sub(232)) * 10;
    gpui::rgb(u32::from(gray) << 16 | u32::from(gray) << 8 | u32::from(gray))
}

fn control_error(code: impl Into<String>, message: impl Into<String>) -> ControlResult {
    ControlResult::Error(ControlErrorInfo::new(code, message))
}

fn capture_screen_png() -> Option<PathBuf> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis();
    let path = std::env::temp_dir().join(format!("agenthouse-surface-{timestamp}.png"));
    let status = Command::new("screencapture")
        .arg("-x")
        .arg(&path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok()?;

    if status.success() && path.exists() {
        Some(path)
    } else {
        None
    }
}

fn write_structured_surface_snapshot(value: serde_json::Value) -> Option<PathBuf> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis();
    let path = std::env::temp_dir().join(format!("agenthouse-surface-{timestamp}.json"));
    let json = serde_json::to_vec_pretty(&value).ok()?;
    fs::write(&path, json).ok()?;
    Some(path)
}

fn sanitize_terminal_block_text(text: &mut String) {
    let mut cleaned = text
        .replace("\u{1b}[?2004h", "")
        .replace("\u{1b}[?2004l", "")
        .replace("\u{1b}[?25h", "")
        .replace("\r\r\n", "\n")
        .replace("\r\n", "\n")
        .replace('\r', "\n");

    cleaned = strip_ansi_csi(&cleaned);

    while cleaned.contains("\n\n\n") {
        cleaned = cleaned.replace("\n\n\n", "\n\n");
    }

    *text = cleaned.trim_start_matches('\n').to_string();
}

fn remove_echoed_command_lines(text: &mut String, command: &str) {
    let Some(command_name) = command.split_whitespace().next() else {
        return;
    };

    let mut cleaned = Vec::new();
    let mut skipping_echo = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if cleaned.is_empty() && (trimmed.is_empty() || trimmed == "%") {
            continue;
        }
        let prompt_echo = trimmed.contains(" % ") && trimmed.contains(command_name);
        let raw_echo = trimmed.starts_with(command_name) && trimmed.contains("; printf");
        let wrapped_echo = trimmed.starts_with('<');

        if prompt_echo || raw_echo || wrapped_echo {
            skipping_echo = true;
            continue;
        }
        if skipping_echo && trimmed.is_empty() {
            continue;
        }
        skipping_echo = false;
        cleaned.push(line);
    }
    while cleaned.first().is_some_and(|line| line.trim().is_empty()) {
        cleaned.remove(0);
    }
    while cleaned.last().is_some_and(|line| line.trim().is_empty()) {
        cleaned.pop();
    }
    *text = cleaned.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
}

fn strip_ansi_csi(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\u{1b}' {
            output.push(ch);
            continue;
        }

        match chars.peek().copied() {
            Some('[') => {
                chars.next();
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
            Some(']') => {
                chars.next();
                let mut previous = '\0';
                for next in chars.by_ref() {
                    if next == '\u{7}' || (previous == '\u{1b}' && next == '\\') {
                        break;
                    }
                    previous = next;
                }
            }
            _ => {}
        }
    }
    output
}

fn retain_recent_utf8(text: &mut String, max_bytes: usize) {
    if text.len() <= max_bytes {
        return;
    }
    if max_bytes == 0 {
        text.clear();
        return;
    }

    let mut keep_from = text.len() - max_bytes;
    while !text.is_char_boundary(keep_from) {
        keep_from += 1;
    }
    text.drain(..keep_from);
}

fn terminal_input_for_command(
    command: &str,
    marker: &CommandCompletionMarker,
) -> std::io::Result<String> {
    let script_path = write_command_script(command, marker)?;
    Ok(format!(
        ". {}; __agenthouse_status=$?; rm -f {}; true\n",
        shell_quote_path(&script_path),
        shell_quote_path(&script_path)
    ))
}

fn write_command_script(
    command: &str,
    marker: &CommandCompletionMarker,
) -> std::io::Result<PathBuf> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let path = std::env::temp_dir().join(format!(
        "agenthouse-command-{}-{timestamp}.sh",
        marker.sequence
    ));
    fs::write(
        &path,
        format!(
            "export COLUMNS={TERMINAL_GRID_COLS} LINES={TERMINAL_GRID_ROWS}\nstty cols {TERMINAL_GRID_COLS} rows {TERMINAL_GRID_ROWS} 2>/dev/null || true\nprintf '\\r\\n{}\\r\\n'\n(\n{command}\n)\n__agenthouse_status=$?\nprintf '\\r\\n{}%s\\r\\n' \"$__agenthouse_status\"\nreturn \"$__agenthouse_status\" 2>/dev/null || exit \"$__agenthouse_status\"\n",
            marker.begin_prefix, marker.done_prefix
        ),
    )?;
    Ok(path)
}

fn shell_quote_path(path: &Path) -> String {
    let value = path.display().to_string();
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn extract_completion_exit_code(
    text: &mut String,
    marker: &CommandCompletionMarker,
) -> Option<i32> {
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if let Some(code) = completion_exit_code(line, marker) {
            text.truncate(offset);
            return Some(code);
        }
        offset += line.len();
    }
    None
}

fn completion_exit_code(line: &str, marker: &CommandCompletionMarker) -> Option<i32> {
    let line = line.trim_end_matches(['\r', '\n']);
    line.strip_prefix(&marker.done_prefix)?.parse().ok()
}

fn remove_marker_lines(text: &mut String, marker: &CommandCompletionMarker) {
    if !text.contains(&marker.begin_prefix)
        && !text.contains(&marker.done_prefix)
        && !text.contains("\"$?\"")
    {
        return;
    }

    let mut cleaned = String::with_capacity(text.len());
    let mut skip_next_fragment = false;
    for line in text.split_inclusive('\n') {
        if line.contains(&marker.begin_prefix)
            || line.contains(&marker.done_prefix)
            || line.contains("\"$?\"")
        {
            if line.trim_end_matches(['\r', '\n']).ends_with('\'') {
                skip_next_fragment = true;
            }
        } else if skip_next_fragment && line.contains("\"$?\"") {
            skip_next_fragment = false;
        } else {
            skip_next_fragment = false;
            cleaned.push_str(line);
        }
    }
    *text = cleaned;
}

fn discard_until_begin_marker(text: &mut String, marker: &CommandCompletionMarker) -> bool {
    let Some(position) = text.find(&marker.begin_prefix) else {
        return false;
    };
    let after_marker = position + marker.begin_prefix.len();
    let after_line = text[after_marker..]
        .find('\n')
        .map(|offset| after_marker + offset + 1)
        .unwrap_or(after_marker);
    text.drain(..after_line);
    true
}

#[cfg(test)]
mod tests {
    use super::{
        AgentHouseTheme, BrowserRuntime, CommandCompletionMarker, DEFAULT_BROWSER_URL,
        TERMINAL_CELL_HEIGHT_PX, TERMINAL_CELL_WIDTH_PX, TerminalPromptSubmission,
        clean_claude_json_text, clean_forwarded_block_text, clean_workspace_for_ui,
        completion_exit_code, extract_completion_exit_code, file_preview_snapshot,
        finalize_command_block_text, normalize_browser_address, offset_from_utf16, offset_to_utf16,
        pane_grid_columns, pane_grid_rows, range_from_utf16, range_to_utf16,
        remove_echoed_command_lines, remove_marker_lines, retain_recent_utf8,
        sanitize_browser_address_insert_text, sanitize_terminal_block_text, strip_ansi_csi,
        terminal_grid_metrics_for_bounds, terminal_input_for_command, terminal_input_for_display,
        terminal_line_segments, terminal_prompt_submission, workspace_window_for_ui,
    };
    use ah_block::{Block, BlockAttachment, BlockKind};
    use ah_core::{Actor, SessionId};
    use ah_terminal::{TerminalColor, TerminalScreenCell};
    use ah_workspace::LayoutMode;
    use gpui::{bounds, point, px, size};
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn ui_workspace_starts_clean_without_windows() {
        let root = PathBuf::from("/tmp/agenthouse-rs-ui-test");

        let workspace = clean_workspace_for_ui(1, root.clone());

        assert_eq!(workspace.name, "agenthouse-rs-ui-test");
        assert_eq!(workspace.root, Some(root));
        assert!(workspace.windows.is_empty());
        assert_eq!(workspace.active_window_id, None);
    }

    #[test]
    fn ui_window_starts_clean_without_tabs() {
        let window = workspace_window_for_ui(2);

        assert_eq!(window.title, "Window 3");
        assert!(window.tabs.is_empty());
        assert_eq!(window.active_tab_id, None);
    }

    #[test]
    fn pane_grid_columns_follow_manual_split_limits() {
        assert_eq!(pane_grid_columns(1, &LayoutMode::Single), 1);
        assert_eq!(pane_grid_rows(1, 1), 1);
        assert_eq!(pane_grid_columns(2, &LayoutMode::Columns), 2);
        assert_eq!(pane_grid_rows(2, 2), 1);
        assert_eq!(pane_grid_columns(3, &LayoutMode::Columns), 2);
        assert_eq!(pane_grid_rows(3, 2), 2);
        assert_eq!(pane_grid_columns(9, &LayoutMode::Columns), 3);
        assert_eq!(pane_grid_rows(9, 3), 3);
        assert_eq!(pane_grid_columns(16, &LayoutMode::Columns), 4);
        assert_eq!(pane_grid_rows(16, 4), 4);
    }

    #[test]
    fn down_split_prefers_vertical_grid_until_four_panes() {
        assert_eq!(pane_grid_columns(2, &LayoutMode::Grid), 1);
        assert_eq!(pane_grid_rows(2, 1), 2);
        assert_eq!(pane_grid_columns(4, &LayoutMode::Grid), 2);
        assert_eq!(pane_grid_rows(4, 2), 2);
    }

    #[test]
    fn retain_recent_utf8_keeps_valid_suffix() {
        let mut text = "prefix-é-中-🙂-tail".to_string();

        retain_recent_utf8(&mut text, 9);

        assert_eq!(text, "🙂-tail");
    }

    #[test]
    fn retain_recent_utf8_handles_tiny_limit_inside_multibyte_char() {
        let mut text = "🙂".to_string();

        retain_recent_utf8(&mut text, 1);

        assert_eq!(text, "");
    }

    #[test]
    fn terminal_input_adds_completion_marker_after_command() {
        let marker = CommandCompletionMarker::new(7);

        let input = terminal_input_for_command("pwd", &marker);

        let input = input.expect("command script should be created");

        assert!(input.starts_with(". "));
        assert!(input.contains("rm -f "));
        assert!(!input.contains("__AGENTHOUSE_BEGIN_7"));
        assert!(!input.contains("__AGENTHOUSE_DONE_7"));
    }

    #[test]
    fn terminal_grid_metrics_follow_rendered_bounds() {
        let metrics = terminal_grid_metrics_for_bounds(bounds(
            point(px(0.0), px(0.0)),
            size(px(736.0), px(366.0)),
        ))
        .expect("non-empty bounds should produce terminal metrics");

        assert_eq!(
            metrics.cols,
            (736.0 / TERMINAL_CELL_WIDTH_PX).floor() as u16
        );
        assert_eq!(
            metrics.rows,
            (366.0 / TERMINAL_CELL_HEIGHT_PX).floor() as u16
        );
    }

    #[test]
    fn terminal_line_segments_merge_contiguous_cells_with_matching_style() {
        let theme = AgentHouseTheme::glass_magazine();
        let mut bold_b = TerminalScreenCell {
            ch: 'b',
            bold: true,
            ..TerminalScreenCell::default()
        };
        let mut bold_c = TerminalScreenCell {
            ch: 'c',
            bold: true,
            ..TerminalScreenCell::default()
        };
        bold_b.fg = TerminalColor::Named(2);
        bold_c.fg = TerminalColor::Named(2);
        let cells = vec![
            TerminalScreenCell {
                ch: 'a',
                ..TerminalScreenCell::default()
            },
            bold_b,
            bold_c,
            TerminalScreenCell {
                ch: ' ',
                ..TerminalScreenCell::default()
            },
        ];

        let segments = terminal_line_segments(&cells, 0, 9, 9, theme);

        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].text, "a");
        assert_eq!(segments[1].text, "bc");
        assert_eq!(segments[1].cell_count, 2);
        assert!(segments[1].style.bold);
        assert_eq!(segments[2].text, " ");
    }

    #[test]
    fn terminal_line_segments_split_active_cursor_cell() {
        let cells = vec![
            TerminalScreenCell {
                ch: 'a',
                ..TerminalScreenCell::default()
            },
            TerminalScreenCell {
                ch: 'b',
                ..TerminalScreenCell::default()
            },
            TerminalScreenCell {
                ch: 'c',
                ..TerminalScreenCell::default()
            },
        ];

        let segments = terminal_line_segments(&cells, 0, 0, 1, AgentHouseTheme::glass_magazine());

        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].text, "a");
        assert_eq!(segments[1].text, "b");
        assert_eq!(segments[2].text, "c");
        assert_ne!(segments[0].style, segments[1].style);
        assert_eq!(segments[0].style, segments[2].style);
    }

    #[test]
    fn terminal_line_segments_skip_wide_spacers_and_count_wide_cells() {
        let cells = vec![
            TerminalScreenCell {
                ch: '好',
                wide: true,
                ..TerminalScreenCell::default()
            },
            TerminalScreenCell {
                wide_spacer: true,
                ..TerminalScreenCell::default()
            },
            TerminalScreenCell {
                ch: 'x',
                ..TerminalScreenCell::default()
            },
        ];

        let segments = terminal_line_segments(&cells, 0, 9, 9, AgentHouseTheme::glass_magazine());

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "好x");
        assert_eq!(segments[0].cell_count, 3);
    }

    #[test]
    fn file_preview_snapshot_reads_text_file() {
        let path = std::env::temp_dir().join(format!(
            "agenthouse-file-preview-{}.txt",
            std::process::id()
        ));
        fs::write(&path, "AgentHouse file preview\nvisible to agents")
            .expect("test preview file should be writable");

        let preview = file_preview_snapshot(&path);
        let _ = fs::remove_file(&path);

        assert_eq!(preview.kind, "file");
        assert!(!preview.truncated);
        assert!(
            preview
                .text
                .as_deref()
                .is_some_and(|text| text.contains("visible to agents"))
        );
    }

    #[test]
    fn browser_runtime_preserves_failed_backend_open_as_serializable_preview() {
        let mut runtime =
            BrowserRuntime::new_text_preview("Browser", "file:///tmp/agenthouse.html");
        for _ in 0..20 {
            if runtime.drain_events() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let preview = runtime.preview_snapshot();

        assert!(preview.error.is_some());
        assert!(preview.status.contains("browser failed"));
        assert_eq!(preview.http_status, None);

        let value = serde_json::to_value(&preview).expect("web preview should serialize");
        assert_eq!(value["url"], "file:///tmp/agenthouse.html");
        assert!(value["error"].as_str().is_some_and(|error| {
            error.contains("only http://, https://, and about:blank are supported")
        }));
    }

    #[test]
    fn browser_address_normalization_matches_ui_entry_points() {
        assert_eq!(
            normalize_browser_address("example.com"),
            "https://example.com"
        );
        assert_eq!(
            normalize_browser_address(" localhost:3000 "),
            "http://localhost:3000"
        );
        assert_eq!(
            normalize_browser_address("127.0.0.1:8080"),
            "http://127.0.0.1:8080"
        );
        assert_eq!(
            normalize_browser_address("https://zed.dev"),
            "https://zed.dev"
        );
        assert_eq!(normalize_browser_address("about:blank"), "about:blank");
    }

    #[test]
    fn browser_default_url_is_temporary_baidu_smoke_page() {
        assert_eq!(DEFAULT_BROWSER_URL, "https://www.baidu.com/");
    }

    #[test]
    fn browser_address_input_sanitizes_multiline_paste() {
        assert_eq!(
            sanitize_browser_address_insert_text("https://example.com/a\nb\tc\r"),
            "https://example.com/abc"
        );
    }

    #[test]
    fn browser_address_utf16_ranges_preserve_cjk_boundaries() {
        let text = "ab中🙂cd";
        let utf8_range = range_from_utf16(text, &(2..5));

        assert_eq!(&text[utf8_range.clone()], "中🙂");
        assert_eq!(range_to_utf16(text, &utf8_range), 2..5);
        assert_eq!(offset_from_utf16(text, 3), "ab中".len());
        assert_eq!(offset_to_utf16(text, "ab中".len()), 3);
    }

    #[test]
    fn prompt_submission_runs_trimmed_command_when_idle() {
        assert_eq!(
            terminal_prompt_submission("  pwd  ", false),
            Some(TerminalPromptSubmission::Command("pwd".to_string()))
        );
    }

    #[test]
    fn prompt_submission_ignores_empty_command_when_idle() {
        assert_eq!(terminal_prompt_submission("   ", false), None);
    }

    #[test]
    fn prompt_submission_writes_stdin_when_command_is_running() {
        assert_eq!(
            terminal_prompt_submission("hello", true),
            Some(TerminalPromptSubmission::Stdin("hello\n".to_string()))
        );
        assert_eq!(
            terminal_prompt_submission("", true),
            Some(TerminalPromptSubmission::Stdin("\n".to_string()))
        );
    }

    #[test]
    fn terminal_input_for_display_escapes_control_whitespace() {
        assert_eq!(
            terminal_input_for_display("hello\tthere\r\n"),
            "hello\\tthere\\r\\n"
        );
    }

    #[test]
    fn command_script_contains_begin_and_done_markers() {
        let marker = CommandCompletionMarker::new(8);
        let path = super::write_command_script("echo ok", &marker)
            .expect("command script should be created");
        let script = std::fs::read_to_string(&path).expect("command script should be readable");
        let _ = std::fs::remove_file(path);

        assert!(script.contains("__AGENTHOUSE_BEGIN_8"));
        assert!(script.contains("echo ok"));
        assert!(script.contains("__AGENTHOUSE_DONE_8"));
    }

    #[test]
    fn completion_marker_ignores_echoed_command_line() {
        let marker = CommandCompletionMarker::new(7);

        assert_eq!(
            completion_exit_code(
                "printf '\\r\\n__AGENTHOUSE_DONE_7:%s\\r\\n' \"$?\"\r\n",
                &marker
            ),
            None
        );
        assert_eq!(
            completion_exit_code("__AGENTHOUSE_DONE_7:0\r\n", &marker),
            Some(0)
        );
    }

    #[test]
    fn extract_completion_truncates_at_marker_line() {
        let marker = CommandCompletionMarker::new(11);
        let mut text =
            "pwd\r\n/workspace/AgentHouse-RS\r\n__AGENTHOUSE_DONE_11:0\r\n% ".to_string();

        let exit_code = extract_completion_exit_code(&mut text, &marker);

        assert_eq!(exit_code, Some(0));
        assert_eq!(text, "pwd\r\n/workspace/AgentHouse-RS\r\n");
    }

    #[test]
    fn remove_marker_lines_removes_shell_echo_and_marker() {
        let marker = CommandCompletionMarker::new(3);
        let mut text = "pwd; printf '\\r\\n__AGENTHOUSE_DONE_3:%s\\r\\n' \"$?\"\r\n/tmp\r\n__AGENTHOUSE_DONE_3:0\r\n% "
            .to_string();

        remove_marker_lines(&mut text, &marker);

        assert_eq!(text, "/tmp\r\n% ");
    }

    #[test]
    fn remove_marker_lines_removes_wrapped_shell_echo() {
        let marker = CommandCompletionMarker::new(3);
        let mut text = "s\\r\\n' \"$?\"\u{1b}[?2004l\r\r\nAgentHouse-control-smoke\r\n__AGENTHOUSE_DONE_3:0\r\n"
            .to_string();

        remove_marker_lines(&mut text, &marker);

        assert_eq!(text, "AgentHouse-control-smoke\r\n");
    }

    #[test]
    fn discard_until_begin_marker_removes_prior_shell_echo() {
        let marker = CommandCompletionMarker::new(9);
        let mut text = "% noisy prompt\r\n__AGENTHOUSE_BEGIN_9\r\nreal output\r\n".to_string();

        assert!(super::discard_until_begin_marker(&mut text, &marker));
        assert_eq!(text, "real output\r\n");
    }

    #[test]
    fn sanitize_terminal_block_text_removes_ansi_and_extra_cr() {
        let mut text = "\u{1b}[?2004l\r\r\nAgentHouse-control-smoke-2\r\n\r\r\n".to_string();

        sanitize_terminal_block_text(&mut text);

        assert_eq!(text, "AgentHouse-control-smoke-2\n\n");
    }

    #[test]
    fn strip_ansi_csi_removes_common_terminal_controls() {
        let text = "ok\u{1b}[?1006l\u{1b}[31m red\u{1b}[0m\u{1b}]0;title\u{7}\n";

        assert_eq!(strip_ansi_csi(text), "ok red\n");
    }

    #[test]
    fn remove_echoed_command_lines_keeps_real_command_output() {
        let mut text = "%                                                                              \n \n\nHubert@host repo % printf '%s\\n' 'AgentHouse-platform-loop'\n<rintf '%s\\n' 'AgentHouse-platform-loop                                       \n\nAgentHouse-platform-loop\n\n"
            .to_string();

        remove_echoed_command_lines(&mut text, "printf '%s\\n' 'AgentHouse-platform-loop'");

        assert_eq!(text, "AgentHouse-platform-loop\n");
    }

    #[test]
    fn clean_forwarded_block_text_prefers_result_json() {
        let text = r#"{"type":"result","result":"clean text\nMARKER"}"#;

        assert_eq!(clean_forwarded_block_text(text), "clean text\nMARKER");
    }

    #[test]
    fn clean_forwarded_block_text_collects_assistant_text() {
        let text = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"first "},{"type":"text","text":"second"}]}}"#;

        assert_eq!(clean_forwarded_block_text(text), "first second");
    }

    #[test]
    fn clean_claude_json_text_collects_stream_delta_text() {
        let text = concat!(
            r#"{"type":"stream_event","event":{"delta":{"text":"first "}}}"#,
            "\n",
            r#"{"type":"stream_event","event":{"delta":{"text":"second"}}}"#,
        );

        assert_eq!(
            clean_claude_json_text(text),
            Some("first second".to_string())
        );
    }

    #[test]
    fn finalize_command_block_text_attaches_raw_claude_output() {
        let raw = r#"{"type":"result","result":"clean answer\nAH_MARKER"}"#;
        let mut block = Block::new(
            SessionId::new(),
            Actor::Human,
            BlockKind::Command,
            raw.to_string(),
        );

        finalize_command_block_text(&mut block, "claude --bare -p prompt");

        assert_eq!(block.text, "clean answer\nAH_MARKER\n");
        let Some(BlockAttachment::File { path }) = block.attachments.first() else {
            panic!("raw Claude output should be attached as a file");
        };
        assert!(path.exists());
        let stored = std::fs::read_to_string(path).expect("raw attachment should be readable");
        assert_eq!(stored, raw);
        let _ = std::fs::remove_file(path);
    }
}
