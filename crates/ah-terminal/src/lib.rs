use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::thread;

use ah_core::{SessionId, Timestamp};
use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::{
    Color as AlacrittyColor, NamedColor, Processor, Rgb as AlacrittyRgb,
};
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc;

const DEFAULT_TERMINAL_COLS: usize = 80;
const DEFAULT_TERMINAL_ROWS: usize = 24;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TerminalCommand {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub cols: u16,
    pub rows: u16,
}

impl TerminalCommand {
    #[must_use]
    pub fn shell(program: impl Into<String>, cwd: PathBuf) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd,
            cols: 80,
            rows: 24,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum TerminalEvent {
    Output {
        session_id: SessionId,
        bytes: Vec<u8>,
        at: Timestamp,
    },
    Exited {
        session_id: SessionId,
        code: Option<i32>,
        at: Timestamp,
    },
}

#[derive(Clone, Copy, Debug, Default)]
struct EmulatorEventListener;

impl EventListener for EmulatorEventListener {
    fn send_event(&self, _event: Event) {}
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TerminalSize {
    cols: usize,
    rows: usize,
}

impl TerminalSize {
    fn new(cols: usize, rows: usize) -> Self {
        Self {
            cols: cols.max(1),
            rows: rows.max(1),
        }
    }
}

impl Dimensions for TerminalSize {
    fn total_lines(&self) -> usize {
        self.screen_lines()
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TerminalScreenSnapshot {
    pub cols: usize,
    pub rows: usize,
    pub cursor_col: usize,
    pub cursor_row: usize,
    pub alt_screen: bool,
    pub lines: Vec<TerminalScreenLine>,
}

impl TerminalScreenSnapshot {
    #[must_use]
    pub fn text(&self) -> String {
        self.lines
            .iter()
            .map(TerminalScreenLine::text)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TerminalScreenLine {
    pub cells: Vec<TerminalScreenCell>,
}

impl TerminalScreenLine {
    #[must_use]
    pub fn text(&self) -> String {
        self.cells.iter().map(|cell| cell.ch).collect::<String>()
    }

    #[must_use]
    pub fn trimmed_text(&self) -> String {
        self.text().trim_end().to_string()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TerminalScreenCell {
    pub ch: char,
    pub fg: TerminalColor,
    pub bg: TerminalColor,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
    pub wide: bool,
    pub wide_spacer: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum TerminalColor {
    DefaultForeground,
    DefaultBackground,
    Named(u8),
    Rgb { r: u8, g: u8, b: u8 },
    Indexed(u8),
}

pub struct TerminalEmulator {
    parser: Processor,
    term: Term<EmulatorEventListener>,
}

impl Default for TerminalEmulator {
    fn default() -> Self {
        Self::new(DEFAULT_TERMINAL_COLS, DEFAULT_TERMINAL_ROWS)
    }
}

impl TerminalEmulator {
    #[must_use]
    pub fn new(cols: usize, rows: usize) -> Self {
        let size = TerminalSize::new(cols, rows);
        let config = Config::default();
        Self {
            parser: Processor::new(),
            term: Term::new(config, &size, EmulatorEventListener),
        }
    }

    pub fn advance(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.term, bytes);
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        let size = TerminalSize::new(cols, rows);
        self.term.resize(size);
    }

    #[must_use]
    pub fn snapshot(&self) -> TerminalScreenSnapshot {
        let grid = self.term.grid();
        let mut lines = vec![
            TerminalScreenLine {
                cells: Vec::with_capacity(grid.columns())
            };
            grid.screen_lines()
        ];

        for indexed in grid.display_iter() {
            let row = indexed.point.line.0;
            if row < 0 {
                continue;
            }
            let row = row as usize;
            let col = indexed.point.column.0;
            if row >= lines.len() {
                continue;
            }
            let cells = &mut lines[row].cells;
            if cells.len() < grid.columns() {
                cells.resize_with(grid.columns(), TerminalScreenCell::default);
            }
            if col < cells.len() {
                cells[col] = terminal_screen_cell(indexed.cell);
            }
        }

        for line in &mut lines {
            if line.cells.len() < grid.columns() {
                line.cells
                    .resize_with(grid.columns(), TerminalScreenCell::default);
            }
        }

        let cursor = grid.cursor.point;
        let cursor_row = cursor.line.0.max(0) as usize;
        TerminalScreenSnapshot {
            cols: grid.columns(),
            rows: grid.screen_lines(),
            cursor_col: cursor.column.0.min(grid.columns().saturating_sub(1)),
            cursor_row: cursor_row.min(grid.screen_lines().saturating_sub(1)),
            alt_screen: self.term.mode().contains(TermMode::ALT_SCREEN),
            lines,
        }
    }

    #[must_use]
    pub fn mode(&self) -> TermMode {
        *self.term.mode()
    }
}

impl Default for TerminalScreenCell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: TerminalColor::DefaultForeground,
            bg: TerminalColor::DefaultBackground,
            bold: false,
            italic: false,
            underline: false,
            inverse: false,
            wide: false,
            wide_spacer: false,
        }
    }
}

fn terminal_screen_cell(cell: &Cell) -> TerminalScreenCell {
    let wide_spacer = cell.flags.contains(Flags::WIDE_CHAR_SPACER);
    TerminalScreenCell {
        ch: if wide_spacer { ' ' } else { cell.c },
        fg: terminal_color(cell.fg, true),
        bg: terminal_color(cell.bg, false),
        bold: cell.flags.contains(Flags::BOLD),
        italic: cell.flags.contains(Flags::ITALIC),
        underline: cell.flags.intersects(Flags::ALL_UNDERLINES),
        inverse: cell.flags.contains(Flags::INVERSE),
        wide: cell.flags.contains(Flags::WIDE_CHAR),
        wide_spacer,
    }
}

fn terminal_color(color: AlacrittyColor, foreground: bool) -> TerminalColor {
    match color {
        AlacrittyColor::Named(NamedColor::Foreground) if foreground => {
            TerminalColor::DefaultForeground
        }
        AlacrittyColor::Named(NamedColor::Background) if !foreground => {
            TerminalColor::DefaultBackground
        }
        AlacrittyColor::Named(named) => TerminalColor::Named(named as u8),
        AlacrittyColor::Spec(AlacrittyRgb { r, g, b }) => TerminalColor::Rgb { r, g, b },
        AlacrittyColor::Indexed(index) => TerminalColor::Indexed(index),
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TerminalKeyModifiers {
    pub alt: bool,
    pub control: bool,
    pub shift: bool,
    pub platform: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalKey {
    pub key: String,
    pub text: Option<String>,
    pub modifiers: TerminalKeyModifiers,
}

impl TerminalKey {
    #[must_use]
    pub fn new(key: impl Into<String>, text: Option<String>) -> Self {
        Self {
            key: key.into(),
            text,
            modifiers: TerminalKeyModifiers::default(),
        }
    }
}

#[must_use]
pub fn input_sequence_for_key(key: &TerminalKey, mode: TermMode) -> Option<String> {
    if key.modifiers.platform {
        return None;
    }

    if key.modifiers.control
        && !key.modifiers.alt
        && let Some(control) = control_sequence(&key.key)
    {
        return Some(control.to_string());
    }

    let sequence = match key.key.as_str() {
        "tab" if key.modifiers.shift => "\x1b[Z",
        "tab" => "\t",
        "escape" => "\x1b",
        "enter" if key.modifiers.shift => "\n",
        "enter" => "\r",
        "backspace" if key.modifiers.control => "\x08",
        "backspace" => "\x7f",
        "delete" => "\x1b[3~",
        "insert" => "\x1b[2~",
        "home" if mode.contains(TermMode::APP_CURSOR) => "\x1bOH",
        "home" => "\x1b[H",
        "end" if mode.contains(TermMode::APP_CURSOR) => "\x1bOF",
        "end" => "\x1b[F",
        "up" if mode.contains(TermMode::APP_CURSOR) => "\x1bOA",
        "up" => "\x1b[A",
        "down" if mode.contains(TermMode::APP_CURSOR) => "\x1bOB",
        "down" => "\x1b[B",
        "right" if mode.contains(TermMode::APP_CURSOR) => "\x1bOC",
        "right" => "\x1b[C",
        "left" if mode.contains(TermMode::APP_CURSOR) => "\x1bOD",
        "left" => "\x1b[D",
        "pageup" => "\x1b[5~",
        "pagedown" => "\x1b[6~",
        "f1" => "\x1bOP",
        "f2" => "\x1bOQ",
        "f3" => "\x1bOR",
        "f4" => "\x1bOS",
        "f5" => "\x1b[15~",
        "f6" => "\x1b[17~",
        "f7" => "\x1b[18~",
        "f8" => "\x1b[19~",
        "f9" => "\x1b[20~",
        "f10" => "\x1b[21~",
        "f11" => "\x1b[23~",
        "f12" => "\x1b[24~",
        _ => {
            return key
                .text
                .as_ref()
                .filter(|text| {
                    !key.modifiers.control
                        && !text.chars().any(char::is_control)
                        && (!key.modifiers.alt || !text.is_empty())
                })
                .map(|text| {
                    if key.modifiers.alt {
                        format!("\x1b{text}")
                    } else {
                        text.clone()
                    }
                });
        }
    };

    Some(if key.modifiers.alt {
        format!("\x1b{sequence}")
    } else {
        sequence.to_string()
    })
}

#[must_use]
pub fn paste_sequence_for_text(text: &str) -> Option<String> {
    let mut sanitized = String::new();
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    for ch in normalized.chars() {
        if ch == '\n' || ch == '\t' || !ch.is_control() {
            sanitized.push(ch);
        }
    }

    if sanitized.is_empty() {
        None
    } else {
        Some(format!("\x1b[200~{sanitized}\x1b[201~"))
    }
}

fn control_sequence(key: &str) -> Option<&'static str> {
    match key {
        "a" | "A" => Some("\x01"),
        "b" | "B" => Some("\x02"),
        "c" | "C" => Some("\x03"),
        "d" | "D" => Some("\x04"),
        "e" | "E" => Some("\x05"),
        "f" | "F" => Some("\x06"),
        "g" | "G" => Some("\x07"),
        "h" | "H" => Some("\x08"),
        "i" | "I" => Some("\x09"),
        "j" | "J" => Some("\x0a"),
        "k" | "K" => Some("\x0b"),
        "l" | "L" => Some("\x0c"),
        "m" | "M" => Some("\x0d"),
        "n" | "N" => Some("\x0e"),
        "o" | "O" => Some("\x0f"),
        "p" | "P" => Some("\x10"),
        "q" | "Q" => Some("\x11"),
        "r" | "R" => Some("\x12"),
        "s" | "S" => Some("\x13"),
        "t" | "T" => Some("\x14"),
        "u" | "U" => Some("\x15"),
        "v" | "V" => Some("\x16"),
        "w" | "W" => Some("\x17"),
        "x" | "X" => Some("\x18"),
        "y" | "Y" => Some("\x19"),
        "z" | "Z" => Some("\x1a"),
        "@" | "space" => Some("\x00"),
        "[" => Some("\x1b"),
        "\\" => Some("\x1c"),
        "]" => Some("\x1d"),
        "^" => Some("\x1e"),
        "_" => Some("\x1f"),
        "?" => Some("\x7f"),
        _ => None,
    }
}

pub struct PtySession {
    session_id: SessionId,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    process_id: Option<u32>,
}

impl PtySession {
    pub fn spawn(
        session_id: SessionId,
        command: &TerminalCommand,
        events: mpsc::UnboundedSender<TerminalEvent>,
    ) -> Result<Self, TerminalError> {
        Self::spawn_with_wake(session_id, command, events, None)
    }

    pub fn spawn_with_wake(
        session_id: SessionId,
        command: &TerminalCommand,
        events: mpsc::UnboundedSender<TerminalEvent>,
        wake: Option<mpsc::UnboundedSender<()>>,
    ) -> Result<Self, TerminalError> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows: command.rows,
            cols: command.cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let mut builder = CommandBuilder::new(&command.program);
        builder.cwd(&command.cwd);
        builder.env("TERM", "xterm-256color");
        builder.env("COLORTERM", "truecolor");
        builder.env("CLICOLOR", "1");
        builder.env("FORCE_COLOR", "1");
        for arg in &command.args {
            builder.arg(arg);
        }

        let mut child = pair.slave.spawn_command(builder)?;
        let killer = child.clone_killer();
        let process_id = child.process_id();
        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        drop(pair.slave);

        thread::spawn(move || {
            let mut buffer = [0_u8; 8192];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => {
                        let sent = events.send(TerminalEvent::Output {
                            session_id,
                            bytes: buffer[..count].to_vec(),
                            at: Timestamp::now(),
                        });
                        if sent.is_ok()
                            && let Some(wake) = &wake
                        {
                            let _ = wake.send(());
                        }
                    }
                    Err(_) => break,
                }
            }

            let code = child
                .wait()
                .ok()
                .and_then(|status| i32::try_from(status.exit_code()).ok());
            let sent = events.send(TerminalEvent::Exited {
                session_id,
                code,
                at: Timestamp::now(),
            });
            if sent.is_ok()
                && let Some(wake) = &wake
            {
                let _ = wake.send(());
            }
        });

        Ok(Self {
            session_id,
            master: pair.master,
            writer,
            killer,
            process_id,
        })
    }

    pub fn write(&mut self, bytes: &[u8]) -> Result<(), TerminalError> {
        self.writer.write_all(bytes)?;
        self.writer.flush()?;
        Ok(())
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), TerminalError> {
        self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(())
    }

    pub fn interrupt(&mut self) -> Result<(), TerminalError> {
        self.write(b"\x03")
    }

    pub fn terminate(&mut self) -> Result<(), TerminalError> {
        self.killer.kill()?;
        Ok(())
    }

    #[must_use]
    pub fn session_id(&self) -> SessionId {
        self.session_id
    }

    #[must_use]
    pub fn process_id(&self) -> Option<u32> {
        self.process_id
    }
}

#[derive(Debug, Error)]
pub enum TerminalError {
    #[error("failed to open pty: {0}")]
    Pty(#[from] anyhow::Error),
    #[error("terminal io failed: {0}")]
    Io(#[from] std::io::Error),
}

pub fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string())
}

pub fn command_for_shell(cwd: &Path) -> TerminalCommand {
    TerminalCommand::shell(default_shell(), cwd.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::{
        TerminalColor, TerminalEmulator, TerminalKey, TerminalKeyModifiers, input_sequence_for_key,
        paste_sequence_for_text,
    };
    use alacritty_terminal::term::TermMode;

    #[test]
    fn emulator_tracks_cursor_moves_and_overwrites_cells() {
        let mut emulator = TerminalEmulator::new(8, 3);

        emulator.advance(b"hello\x1b[1;2H!");
        let snapshot = emulator.snapshot();

        assert_eq!(snapshot.lines[0].trimmed_text(), "h!llo");
        assert_eq!(snapshot.cursor_row, 0);
        assert_eq!(snapshot.cursor_col, 2);
    }

    #[test]
    fn emulator_handles_clear_screen() {
        let mut emulator = TerminalEmulator::new(8, 3);

        emulator.advance(b"hello\x1b[2J\x1b[Hdone");
        let snapshot = emulator.snapshot();

        assert_eq!(snapshot.lines[0].trimmed_text(), "done");
        assert_eq!(snapshot.lines[1].trimmed_text(), "");
    }

    #[test]
    fn emulator_detects_alternate_screen() {
        let mut emulator = TerminalEmulator::new(8, 3);

        emulator.advance(b"\x1b[?1049halt");
        assert!(emulator.snapshot().alt_screen);
        emulator.advance(b"\x1b[?1049l");
        assert!(!emulator.snapshot().alt_screen);
    }

    #[test]
    fn emulator_exports_basic_cell_style() {
        let mut emulator = TerminalEmulator::new(8, 3);

        emulator.advance(b"\x1b[1;31mA");
        let cell = &emulator.snapshot().lines[0].cells[0];

        assert_eq!(cell.ch, 'A');
        assert!(cell.bold);
        assert_eq!(cell.fg, TerminalColor::Named(1));
    }

    #[test]
    fn key_mapping_covers_basic_terminal_keys() {
        assert_eq!(
            input_sequence_for_key(&TerminalKey::new("enter", None), TermMode::empty()),
            Some("\r".to_string())
        );
        assert_eq!(
            input_sequence_for_key(&TerminalKey::new("up", None), TermMode::empty()),
            Some("\x1b[A".to_string())
        );
        assert_eq!(
            input_sequence_for_key(&TerminalKey::new("left", None), TermMode::APP_CURSOR),
            Some("\x1bOD".to_string())
        );
    }

    #[test]
    fn key_mapping_covers_control_and_text_input() {
        let mut ctrl_c = TerminalKey::new("c", None);
        ctrl_c.modifiers = TerminalKeyModifiers {
            control: true,
            ..TerminalKeyModifiers::default()
        };
        assert_eq!(
            input_sequence_for_key(&ctrl_c, TermMode::empty()),
            Some("\x03".to_string())
        );

        let text = TerminalKey::new("a", Some("a".to_string()));
        assert_eq!(
            input_sequence_for_key(&text, TermMode::empty()),
            Some("a".to_string())
        );
    }

    #[test]
    fn paste_sequence_uses_bracketed_paste_and_sanitizes_controls() {
        assert_eq!(
            paste_sequence_for_text("one\r\ntwo\x07\tthree"),
            Some("\x1b[200~one\ntwo\tthree\x1b[201~".to_string())
        );
        assert_eq!(paste_sequence_for_text("\x07"), None);
    }
}
