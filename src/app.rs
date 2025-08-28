use crate::config::Config;
use crate::fs_ops::{
    build_notes_tree, ensure_notes_dir, flatten_tree_for_sidebar, read_note, rename_note,
    write_note, FlatNode,
};

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
use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
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
    // filesystem
    pub notes_dir: PathBuf,

    // sidebar tree (flattened)
    pub sidebar_items: Vec<FlatNode>,
    pub expanded_dirs: HashSet<PathBuf>,
    pub sidebar_state: ListState,

    // editor buffer
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

        let mut expanded_dirs = HashSet::new();
        expanded_dirs.insert(notes_dir.clone()); // root expanded by default

        let sidebar_items = Self::build_sidebar(&notes_dir, &expanded_dirs)?;

        let mut sidebar_state = ListState::default();
        if !sidebar_items.is_empty() {
            sidebar_state.select(Some(0));
        }

        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        Ok(Self {
            notes_dir,
            sidebar_items,
            expanded_dirs,
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
            // avoid self-borrow clash with Terminal::draw closure
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
        // quit
        if key.code == KeyCode::Char('q') && key.modifiers.is_empty() {
            return Ok(true);
        }

        // save
        if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.save_current()?;
            return Ok(false);
        }

        // new draft
        if key.code == KeyCode::Char('n') && key.modifiers.is_empty() {
            self.create_new_note()?;
            return Ok(false);
        }

        // quick focus switch h/l
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
        let len = self.sidebar_items.len();
        let selected = self.sidebar_state.selected().unwrap_or(0);

        match key.code {
            KeyCode::Up => {
                if len > 0 {
                    let new = selected.saturating_sub(1);
                    self.sidebar_state.select(Some(new));
                }
            }
            KeyCode::Down => {
                if len > 0 {
                    let new = (selected + 1).min(len - 1);
                    self.sidebar_state.select(Some(new));
                }
            }
            KeyCode::Enter => {
                self.sidebar_enter_action(selected)?;
            }
            KeyCode::Char(' ') => {
                // Space toggles folders too
                self.sidebar_toggle_dir(selected)?;
            }
            KeyCode::Right => {
                // quick switch to right panel keeping last focus
                self.focus = match self.last_right_focus {
                    RightFocus::Title => Focus::Title,
                    RightFocus::Content => Focus::Content,
                };
            }
            _ => {}
        }

        Ok(())
    }

    fn sidebar_enter_action(&mut self, idx: usize) -> Result<()> {
        if idx >= self.sidebar_items.len() {
            return Ok(());
        }
        if self.sidebar_items[idx].is_dir {
            // toggle expansion
            self.sidebar_toggle_dir(idx)?;
        } else {
            let path = self.sidebar_items[idx].path.clone();
            self.open_file(&path)?;
        }
        Ok(())
    }


    fn sidebar_toggle_dir(&mut self, idx: usize) -> Result<()> {
        if idx >= self.sidebar_items.len() {
            return Ok(());
        }
        let item = &self.sidebar_items[idx];
        if !item.is_dir {
            return Ok(());
        }
        if self.expanded_dirs.contains(&item.path) {
            self.expanded_dirs.remove(&item.path);
        } else {
            self.expanded_dirs.insert(item.path.clone());
        }
        self.refresh_sidebar_preserve_selection(Some(idx));
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
                    self.dirty = true;
                }
            }
            KeyCode::Delete => {
                if self.title_cursor < self.title.len() {
                    self.title.remove(self.title_cursor);
                    self.dirty = true;
                }
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if c != '/' && c != '\\' && c != '.' && c != '\n' && c != '\r' {
                    self.title.insert(self.title_cursor, c);
                    self.title_cursor += 1;
                    self.dirty = true;
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
        let window = 20usize; // heuristic; UI sets precise scroll when rendering
        if self.cursor_row < self.scroll_y {
            self.scroll_y = self.cursor_row;
        } else if self.cursor_row >= self.scroll_y + window {
            self.scroll_y = self.cursor_row + 1 - window;
        }
    }

    fn open_file(&mut self, path: &Path) -> Result<()> {
        let content = read_note(path).unwrap_or_default();
        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();

        self.title = title;
        self.title_cursor = self.title.len();
        self.lines = split_lines_preserve(&content);
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.scroll_y = 0;
        self.opened_path = Some(path.to_path_buf());
        self.dirty = false;
        self.focus = self.last_right_focus.into();
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

        self.focus = Focus::Title;
        self.last_right_focus = RightFocus::Title;
        Ok(())
    }

    fn save_current(&mut self) -> Result<()> {
        if self.title.trim().is_empty() {
            return Ok(()); // ignore saving with empty title
        }
        let new_path = self.notes_dir.join(format!("{}.md", self.title.trim()));
        let content = self.lines.join("\n");

        if let Some(old) = &self.opened_path {
            if *old != new_path {
                write_note(&new_path, &content)?; // write new content
                rename_note(old, &new_path).ok(); // try rename original (best-effort)
            } else {
                write_note(&new_path, &content)?;
            }
        } else {
            write_note(&new_path, &content)?;
        }

        self.opened_path = Some(new_path.clone());
        self.dirty = false;

        // refresh sidebar and select this file
        self.refresh_sidebar_select_path(&new_path);

        Ok(())
    }

    fn refresh_sidebar_select_path(&mut self, path: &Path) {
        self.refresh_sidebar_preserve_selection(None);
        if let Some(idx) = self
            .sidebar_items
            .iter()
            .position(|n| !n.is_dir && n.path == path)
        {
            self.sidebar_state.select(Some(idx));
        }
    }

    fn refresh_sidebar_preserve_selection(&mut self, prefer_idx: Option<usize>) {
        let old_idx = prefer_idx.or(self.sidebar_state.selected());
        self.sidebar_items = Self::build_sidebar(&self.notes_dir, &self.expanded_dirs).unwrap_or_default();
        if !self.sidebar_items.is_empty() {
            let idx = old_idx.unwrap_or(0).min(self.sidebar_items.len() - 1);
            self.sidebar_state.select(Some(idx));
        } else {
            self.sidebar_state.select(None);
        }
    }

    fn build_sidebar(notes_dir: &Path, expanded: &HashSet<PathBuf>) -> Result<Vec<FlatNode>> {
        let tree = build_notes_tree(notes_dir)?;
        Ok(flatten_tree_for_sidebar(&tree, expanded))
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
            ln.pop(); // strip trailing \n; reconstruct on join
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
