use crate::config::Config;
use crate::fs_ops::{ensure_notes_dir, list_notes, read_note, rename_note, write_note, NoteMeta};

use anyhow::Result;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::widgets::ListState;
use ratatui::Terminal;
use std::io;
use std::path::PathBuf;
use time::macros::format_description;
use time::OffsetDateTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Sidebar,
    Title,
    Content,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RightFocus {
    Title,
    Content,
}

pub struct App {
    // config & fs
    pub notes_dir: PathBuf,
    pub notes: Vec<NoteMeta>,
    // sidebar
    pub sidebar_state: ListState,
    // open buffer
    pub title: String,
    pub title_cursor: usize,
    pub lines: Vec<String>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub scroll_y: usize,
    pub opened_path: Option<PathBuf>,
    pub dirty: bool,
    // focus
    pub focus: Focus,
    pub last_right_focus: RightFocus,
    // terminal
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl App {
    pub fn new(config: Config) -> Result<Self> {
        let notes_dir = config.notes_path();
        ensure_notes_dir(&notes_dir)?;

        let notes = list_notes(&notes_dir)?;
        let mut sidebar_state = ListState::default();
        if !notes.is_empty() {
            sidebar_state.select(Some(0));
        }

        // terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        Ok(Self {
            notes_dir,
            notes,
            sidebar_state,
            title: String::new(),
            title_cursor: 0,
            lines: vec![String::new()],
            cursor_row: 0,
            cursor_col: 0,
            scroll_y: 0,
            opened_path: None,
            dirty: false,
            focus: Focus::Sidebar,
            last_right_focus: RightFocus::Title,
            terminal,
        })
    }

    pub fn run(&mut self) -> Result<()> {
        let res = self.event_loop();

        // restore terminal
        disable_raw_mode()?;
        execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        self.terminal.show_cursor()?;

        res
    }

    fn event_loop(&mut self) -> Result<()> {
        loop {
            let self_ptr: *mut App = self;
            self.terminal.draw(|f| {
                let app: &mut App = unsafe { &mut *self_ptr };
                crate::ui::draw(f, app);
            })?;

            if event::poll(std::time::Duration::from_millis(200))? {
                match event::read()? {
                    Event::Key(k) => {
                        if self.handle_key(k)? {
                            break;
                        }
                    }
                    Event::Resize(_, _) => {
                        self.ensure_cursor_visible();
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<bool> {
        // global quit
        if key.code == KeyCode::Char('q') && key.modifiers.is_empty() {
            return Ok(true);
        }

        // global save
        if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.save_current()?;
            return Ok(false);
        }

        // global new note
        if key.code == KeyCode::Char('n') && key.modifiers.is_empty() {
            self.create_new_note()?;
            return Ok(false);
        }

        // global focus quick switch with h/l
        if key.modifiers.is_empty() {
            match key.code {
                KeyCode::Char('h') => {
                    self.focus = Focus::Sidebar;
                }
                KeyCode::Char('l') => {
                    self.focus = match self.last_right_focus {
                        RightFocus::Title => Focus::Title,
                        RightFocus::Content => Focus::Content,
                    };
                }
                _ => {}
            }
        }

        // Tab cycles focus
        if key.code == KeyCode::Tab {
            self.focus = match self.focus {
                Focus::Sidebar => {
                    self.last_right_focus = RightFocus::Title;
                    Focus::Title
                }
                Focus::Title => {
                    self.last_right_focus = RightFocus::Content;
                    Focus::Content
                }
                Focus::Content => Focus::Sidebar,
            };
            return Ok(false);
        }

        match self.focus {
            Focus::Sidebar => self.handle_sidebar_key(key)?,
            Focus::Title => self.handle_title_key(key)?,
            Focus::Content => self.handle_content_key(key)?,
        }

        Ok(false)
    }

    fn handle_sidebar_key(&mut self, key: KeyEvent) -> Result<()> {
        let selected = self.sidebar_state.selected();
        match key.code {
            KeyCode::Up => {
                if let Some(i) = selected {
                    let new = i.saturating_sub(1);
                    self.sidebar_state.select(Some(new));
                }
            }
            KeyCode::Down => {
                if let Some(i) = selected {
                    if i + 1 < self.notes.len() {
                        self.sidebar_state.select(Some(i + 1));
                    }
                } else if !self.notes.is_empty() {
                    self.sidebar_state.select(Some(0));
                }
            }
            KeyCode::Enter => {
                self.open_selected()?;
            }
            KeyCode::Right => {
                // quick switch to right
                self.focus = match self.last_right_focus {
                    RightFocus::Title => Focus::Title,
                    RightFocus::Content => Focus::Content,
                };
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_title_key(&mut self, key: KeyEvent) -> Result<()> {
        self.last_right_focus = RightFocus::Title;
        match key.code {
            KeyCode::Left => {
                if self.title_cursor > 0 {
                    self.title_cursor -= 1;
                }
            }
            KeyCode::Right => {
                if self.title_cursor < self.title.len() {
                    self.title_cursor += 1;
                }
            }
            KeyCode::Home => {
                self.title_cursor = 0;
            }
            KeyCode::End => {
                self.title_cursor = self.title.len();
            }
            KeyCode::Backspace => {
                if self.title_cursor > 0 {
                    self.title.remove(self.title_cursor - 1);
                    self.title_cursor -= 1;
                }
            }
            KeyCode::Delete => {
                if self.title_cursor < self.title.len() {
                    self.title.remove(self.title_cursor);
                }
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if c != '/' && c != '\\' && c != '.' && c != '\n' && c != '\r' {
                    self.title.insert(self.title_cursor, c);
                    self.title_cursor += 1;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_content_key(&mut self, key: KeyEvent) -> Result<()> {
        self.last_right_focus = RightFocus::Content;
        match key.code {
            KeyCode::Left => {
                if self.cursor_col > 0 {
                    self.cursor_col -= 1;
                } else if self.cursor_row > 0 {
                    self.cursor_row -= 1;
                    self.cursor_col = self.lines[self.cursor_row].len();
                }
            }
            KeyCode::Right => {
                if self.cursor_col < self.lines[self.cursor_row].len() {
                    self.cursor_col += 1;
                } else if self.cursor_row + 1 < self.lines.len() {
                    self.cursor_row += 1;
                    self.cursor_col = 0;
                }
            }
            KeyCode::Up => {
                if self.cursor_row > 0 {
                    self.cursor_row -= 1;
                    self.cursor_col = self.cursor_col.min(self.lines[self.cursor_row].len());
                }
            }
            KeyCode::Down => {
                if self.cursor_row + 1 < self.lines.len() {
                    self.cursor_row += 1;
                    self.cursor_col = self.cursor_col.min(self.lines[self.cursor_row].len());
                }
            }
            KeyCode::Home => {
                self.cursor_col = 0;
            }
            KeyCode::End => {
                self.cursor_col = self.lines[self.cursor_row].len();
            }
            KeyCode::Backspace => {
                if self.cursor_col > 0 {
                    self.lines[self.cursor_row].remove(self.cursor_col - 1);
                    self.cursor_col -= 1;
                } else if self.cursor_row > 0 {
                    let prev_len = self.lines[self.cursor_row - 1].len();
                    let curr = self.lines.remove(self.cursor_row);
                    self.cursor_row -= 1;
                    self.cursor_col = prev_len;
                    self.lines[self.cursor_row].push_str(&curr);
                }
                self.dirty = true;
            }
            KeyCode::Delete => {
                if self.cursor_col < self.lines[self.cursor_row].len() {
                    self.lines[self.cursor_row].remove(self.cursor_col);
                } else if self.cursor_row + 1 < self.lines.len() {
                    let next = self.lines.remove(self.cursor_row + 1);
                    self.lines[self.cursor_row].push_str(&next);
                }
                self.dirty = true;
            }
            KeyCode::Enter => {
                let rest = self.lines[self.cursor_row].split_off(self.cursor_col);
                self.cursor_row += 1;
                self.cursor_col = 0;
                self.lines.insert(self.cursor_row, rest);
                self.dirty = true;
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.lines[self.cursor_row].insert(self.cursor_col, c);
                self.cursor_col += 1;
                self.dirty = true;
            }
            _ => {}
        }
        self.ensure_cursor_visible();
        Ok(())
    }

    fn ensure_cursor_visible(&mut self) {
        // simple vertical scroll: keep cursor within viewport window height guess
        // This function will be called after drawing too, but we don't know exact area height here.
        // Use a heuristic window of 20 lines; it's corrected by UI via paragraph scroll.
        let window = 20usize;
        if self.cursor_row < self.scroll_y {
            self.scroll_y = self.cursor_row;
        } else if self.cursor_row >= self.scroll_y + window {
            self.scroll_y = self.cursor_row + 1 - window;
        }
    }

    fn open_selected(&mut self) -> Result<()> {
        if let Some(i) = self.sidebar_state.selected() {
            if let Some(meta) = self.notes.get(i) {
                let content = read_note(&meta.path).unwrap_or_default();
                self.title = meta.title.clone();
                self.title_cursor = self.title.len();
                self.lines = split_lines_preserve(&content);
                if self.lines.is_empty() {
                    self.lines.push(String::new());
                }
                self.cursor_row = 0;
                self.cursor_col = 0;
                self.scroll_y = 0;
                self.opened_path = Some(meta.path.clone());
                self.dirty = false;
                self.focus = self.last_right_focus.into();
            }
        }
        Ok(())
    }

    fn create_new_note(&mut self) -> Result<()> {
        let ts = OffsetDateTime::now_local()
            .unwrap_or_else(|_| OffsetDateTime::now_utc())
            .format(&format_description!("[year][month][day]-[hour][minute][second]"))?;
        let draft = format!("note-{}", ts);

        self.title = draft.clone();
        self.title_cursor = self.title.len();
        self.lines = vec![String::new()];
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.scroll_y = 0;
        self.opened_path = None;
        self.dirty = true;

        // Add to notes list if not present
        let path = self.notes_dir.join(format!("{}.md", self.title));
        let meta = NoteMeta {
            title: self.title.clone(),
            path,
        };
        self.notes.push(meta);
        self.notes.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        // select it
        if let Some(pos) = self.notes.iter().position(|n| n.title == self.title) {
            self.sidebar_state.select(Some(pos));
        }
        self.focus = Focus::Title;
        self.last_right_focus = RightFocus::Title;
        Ok(())
    }

    fn save_current(&mut self) -> Result<()> {
        if self.title.trim().is_empty() {
            return Ok(()); // ignore saving empty title
        }
        let new_path = self.notes_dir.join(format!("{}.md", self.title.trim()));
        let content = self.lines.join("\n");

        // If previously opened another path, rename if different
        if let Some(old) = &self.opened_path {
            if *old != new_path {
                // Ensure no collision: if new_path exists and not the same file, we will overwrite
                // (simple approach). Could also disallow; for now overwrite.
                write_note(&new_path, &content)?;
                rename_note(old, &new_path).ok(); // best-effort
            } else {
                write_note(&new_path, &content)?;
            }
        } else {
            write_note(&new_path, &content)?;
        }

        self.opened_path = Some(new_path.clone());
        self.dirty = false;

        // Update notes list
        let title_now = self.title.clone();
        self.notes = list_notes(&self.notes_dir)?;
        if let Some(pos) = self.notes.iter().position(|n| n.title == title_now) {
            self.sidebar_state.select(Some(pos));
        }

        Ok(())
    }
}

impl From<RightFocus> for Focus {
    fn from(value: RightFocus) -> Self {
        match value {
            RightFocus::Title => Focus::Title,
            RightFocus::Content => Focus::Content,
        }
    }
}

fn split_lines_preserve(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (_i, line) in s.split_inclusive('\n').enumerate() {
        if line.ends_with('\n') {
            let mut ln = line.to_string();
            ln.pop(); // remove trailing \n, will be reconstructed by join
            out.push(ln);
        } else {
            out.push(line.to_string());
        }
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}
