mod control_server;
mod shell;

use std::borrow::Cow;
use std::path::PathBuf;

use ah_control::{UiLanguagePreference, UiThemeSchemePreference};
use gpui::{
    App, AppContext, Bounds, Menu, MenuItem, SystemMenuType, TitlebarOptions, WindowBounds,
    WindowOptions, point, px, size,
};
use gpui_platform::application;
use tokio::sync::mpsc;

use crate::control_server::start_control_server;
use crate::shell::{
    AboutAgentHouse, AgentHouseShell, CheckForUpdates, CloseWorkspace, NewTerminalTab, NewWebTab,
    OpenSettings, OpenWorkspaceFolder, QuitAgentHouse, RenameWorkspace, SetLanguageChinese,
    SetLanguageEnglish, SetThemeBlue, SetThemeCream, SetThemeGlass, SetThemeGreen, SetThemeLuxury,
    SetThemePurple, SetThemeRed, SetThemeSoft, SetThemeWarm, SplitWindowDown, SplitWindowRight,
};

const DESIGN_FONT_BYTES: [&[u8]; 4] = [
    include_bytes!("../assets/fonts/geist/geist-latin.woff2"),
    include_bytes!("../assets/fonts/geist/geist-latin-ext.woff2"),
    include_bytes!("../assets/fonts/geist/geist-mono-latin.woff2"),
    include_bytes!("../assets/fonts/geist/geist-mono-latin-ext.woff2"),
];
const APP_ICON_BYTES: &[u8] = include_bytes!("../assets/app-icon.jpg");
const DESIGN_TRAFFIC_LIGHT_X_PX: f32 = 12.0;
const DESIGN_TRAFFIC_LIGHT_Y_PX: f32 = 10.0;

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub app_name: String,
    pub store_path: PathBuf,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            app_name: "AgentHouse".to_string(),
            store_path: default_store_path(std::env::var_os("AGENTHOUSE_STORE_PATH")),
        }
    }
}

fn default_store_path(env_path: Option<std::ffi::OsString>) -> PathBuf {
    env_path
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("agenthouse-rs.sqlite"))
}

pub fn run(config: AppConfig) -> anyhow::Result<()> {
    tracing::info!(
        app_name = %config.app_name,
        app_icon_bytes = APP_ICON_BYTES.len(),
        "starting AgentHouse"
    );
    let (control_tx, control_rx) = mpsc::unbounded_channel();
    match start_control_server(control_tx) {
        Ok(path) => tracing::info!(path = %path.display(), "control server started"),
        Err(error) => tracing::warn!(?error, "control server did not start"),
    }

    let mut control_rx = Some(control_rx);
    application().run(move |cx: &mut App| {
        register_design_fonts(cx);
        register_app_menu(cx);

        let bounds = Bounds::centered(None, size(gpui::px(1280.0), gpui::px(820.0)), cx);
        let Some(control_rx) = control_rx.take() else {
            tracing::error!("control receiver was already consumed");
            return;
        };
        let result = cx.open_window(design_window_options(bounds), |window, cx| {
            cx.new(|cx| {
                cx.observe_window_appearance(window, |_, window, _| {
                    window.refresh();
                })
                .detach();
                AgentHouseShell::new(cx, control_rx, config.store_path.clone())
            })
        });

        if let Err(error) = result {
            tracing::error!(?error, "failed to open AgentHouse window");
            return;
        }

        cx.activate(true);
    });
    Ok(())
}

fn register_app_menu(cx: &mut App) {
    cx.on_action(|_: &AboutAgentHouse, _cx| {
        tracing::info!(
            version = env!("CARGO_PKG_VERSION"),
            license = env!("CARGO_PKG_LICENSE"),
            "About AgentHouse requested"
        );
    });
    cx.on_action(|_: &CheckForUpdates, _cx| {
        tracing::info!("GitHub update check is reserved for the public alpha");
    });
    cx.on_action(|_: &OpenSettings, _cx| {
        tracing::info!("Settings requested from menu");
    });
    cx.on_action(|_: &SetLanguageChinese, _cx| {
        tracing::info!("Chinese language menu action dispatched");
    });
    cx.on_action(|_: &SetLanguageEnglish, _cx| {
        tracing::info!("English language menu action dispatched");
    });
    cx.on_action(|_: &SetThemeCream, _cx| {
        tracing::info!("cream UI color menu action dispatched");
    });
    cx.on_action(|_: &SetThemeWarm, _cx| {
        tracing::info!("warm UI color menu action dispatched");
    });
    cx.on_action(|_: &SetThemeBlue, _cx| {
        tracing::info!("blue UI color menu action dispatched");
    });
    cx.on_action(|_: &SetThemeGreen, _cx| {
        tracing::info!("green UI color menu action dispatched");
    });
    cx.on_action(|_: &SetThemeRed, _cx| {
        tracing::info!("red UI color menu action dispatched");
    });
    cx.on_action(|_: &SetThemePurple, _cx| {
        tracing::info!("purple UI color menu action dispatched");
    });
    cx.on_action(|_: &SetThemeGlass, _cx| {
        tracing::info!("glass UI color menu action dispatched");
    });
    cx.on_action(|_: &SetThemeLuxury, _cx| {
        tracing::info!("luxury UI color menu action dispatched");
    });
    cx.on_action(|_: &SetThemeSoft, _cx| {
        tracing::info!("soft UI color menu action dispatched");
    });
    cx.on_action(|_: &QuitAgentHouse, cx| {
        tracing::info!("quitting AgentHouse from application menu");
        cx.quit();
    });

    refresh_app_menu(
        cx,
        UiThemeSchemePreference::Glass,
        UiLanguagePreference::ZhCn,
    );
}

pub(crate) fn refresh_app_menu(
    cx: &mut App,
    theme_scheme: UiThemeSchemePreference,
    language: UiLanguagePreference,
) {
    cx.set_menus([
        Menu::new("AgentHouse").items([
            MenuItem::action("关于 AgentHouse", AboutAgentHouse),
            MenuItem::action("检测更新", CheckForUpdates),
            MenuItem::separator(),
            MenuItem::submenu(
                Menu::new("设置").items([
                    MenuItem::submenu(
                        Menu::new("颜色").items([
                            MenuItem::action("宣纸", SetThemeCream)
                                .checked(theme_scheme == UiThemeSchemePreference::Cream),
                            MenuItem::action("暖黄", SetThemeWarm)
                                .checked(theme_scheme == UiThemeSchemePreference::Warm),
                            MenuItem::action("蓝", SetThemeBlue)
                                .checked(theme_scheme == UiThemeSchemePreference::Blue),
                            MenuItem::action("绿", SetThemeGreen)
                                .checked(theme_scheme == UiThemeSchemePreference::Green),
                            MenuItem::action("红", SetThemeRed)
                                .checked(theme_scheme == UiThemeSchemePreference::Red),
                            MenuItem::action("紫", SetThemePurple)
                                .checked(theme_scheme == UiThemeSchemePreference::Purple),
                            MenuItem::action("杂志 / Glass", SetThemeGlass)
                                .checked(theme_scheme == UiThemeSchemePreference::Glass),
                            MenuItem::action("奢华", SetThemeLuxury)
                                .checked(theme_scheme == UiThemeSchemePreference::Luxury),
                            MenuItem::action("柔", SetThemeSoft)
                                .checked(theme_scheme == UiThemeSchemePreference::Soft),
                        ]),
                    ),
                    MenuItem::submenu(Menu::new("外观").items([
                        MenuItem::action("浅色", gpui::NoAction).checked(true),
                        MenuItem::action("深色", gpui::NoAction).disabled(true),
                        MenuItem::action("跟随系统", gpui::NoAction).disabled(true),
                    ])),
                    MenuItem::submenu(
                        Menu::new("语言").items([
                            MenuItem::action("中文", SetLanguageChinese)
                                .checked(language == UiLanguagePreference::ZhCn),
                            MenuItem::action("English", SetLanguageEnglish)
                                .checked(language == UiLanguagePreference::En),
                        ]),
                    ),
                    MenuItem::action("设置...", OpenSettings).disabled(true),
                ]),
            ),
            MenuItem::os_submenu("Services", SystemMenuType::Services),
            MenuItem::separator(),
            MenuItem::action("退出 AgentHouse", QuitAgentHouse),
        ]),
        Menu::new("工作区").items([
            MenuItem::action("打开文件夹作为工作区...", OpenWorkspaceFolder),
            MenuItem::action("重命名当前工作区", RenameWorkspace),
            MenuItem::action("关闭当前工作区", CloseWorkspace),
        ]),
        Menu::new("窗口").items([
            MenuItem::action("新建终端标签页", NewTerminalTab),
            MenuItem::action("新建网页标签页", NewWebTab),
            MenuItem::separator(),
            MenuItem::action("左右分屏", SplitWindowRight),
            MenuItem::action("上下分屏", SplitWindowDown),
        ]),
    ]);
}

fn design_window_options(bounds: Bounds<gpui::Pixels>) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("AgentHouse".into()),
            appears_transparent: true,
            traffic_light_position: Some(point(
                px(DESIGN_TRAFFIC_LIGHT_X_PX),
                px(DESIGN_TRAFFIC_LIGHT_Y_PX),
            )),
        }),
        ..Default::default()
    }
}

fn register_design_fonts(cx: &mut App) {
    let fonts = DESIGN_FONT_BYTES
        .iter()
        .map(|font| Cow::Borrowed(*font))
        .collect::<Vec<Cow<'static, [u8]>>>();

    if let Err(error) = cx.text_system().add_fonts(fonts) {
        tracing::warn!(?error, "failed to register bundled AgentHouse UI fonts");
    }
}
