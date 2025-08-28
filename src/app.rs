use crate::config::Config;
use crate::fs::{
    build_notes_tree, ensure_notes_dir, flatten_tree_for_sidebar, read_note, rename_note,
    write_note, FlatNode,
};
use crate::git::GitSection;

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
 

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Sidebar,
    Title,
    Content,
    Commits,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RightFocus {
    Title,
    Content,
}

#[derive(Debug, Clone)]
pub enum Modal {
    ConfirmDelete { path: PathBuf },
    InputName { current: String, target_dir: PathBuf },
}

pub struct App {
    pub notes_dir: PathBuf,

    pub sidebar_items: Vec<FlatNode>,
    pub expanded_dirs: HashSet<PathBuf>,
    pub sidebar_state: ListState,

    pub title: String,
    pub title_cursor: usize,
    pub lines: Vec<String>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub scroll_y: usize,
    pub opened_path: Option<PathBuf>,
    pub dirty: bool,

    pub focus: Focus,
    pub last_right_focus: RightFocus,

    terminal: Terminal<CrosstermBackend<io::Stdout>>,

    pub git_section: GitSection,
    pub status_message: Option<String>,
    pub new_note_dir: Option<PathBuf>,
    pub modal: Option<Modal>,
}

impl App {
    pub fn new(config: Config) -> Result<Self> {
        let notes_dir = config.notes_path();
        ensure_notes_dir(&notes_dir)?;

        let mut expanded_dirs = HashSet::new();
        expanded_dirs.insert(notes_dir.clone());

        let sidebar_items = Self::build_sidebar(&notes_dir, &expanded_dirs)?;

        let git_section = GitSection::new_for(Some(notes_dir.clone()));

        let mut sidebar_state = ListState::default();
        if !sidebar_items.is_empty() {
            sidebar_state.select(Some(0));
        }

        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        let mut app = Self {
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
            git_section,
            status_message: None,
            new_note_dir: None,
            modal: None,
        };

        if app.git_section.commits.is_empty() {
            app.status_message = Some("No commits found in notes folder or git not initialized".to_string());
        }

        Ok(app)
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
        if self.modal.is_some() {
            self.handle_modal_key(key)?;
            return Ok(false);
        }

        if key.modifiers.is_empty() {
            match key.code {
                KeyCode::Char('1') => { self.focus = Focus::Sidebar; return Ok(false); }
                KeyCode::Char('2') => { self.focus = Focus::Title; return Ok(false); }
                KeyCode::Char('3') => { self.focus = Focus::Content; return Ok(false); }
                KeyCode::Char('4') => { self.focus = Focus::Commits; return Ok(false); }
                _ => {}
            }
        }

        if key.code == KeyCode::Char('q') && key.modifiers.is_empty() {
            return Ok(true);
        }

        if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.save_current()?;
            return Ok(false);
        }
        if key.code == KeyCode::Char('n') && key.modifiers.is_empty() {
            let mut target = self.notes_dir.clone();
            if matches!(self.focus, Focus::Sidebar) {
                if let Some(sel) = self.sidebar_state.selected() {
                    if sel < self.sidebar_items.len() {
                        let it = &self.sidebar_items[sel];
                        if it.is_dir {
                            target = it.path.clone();
                        } else if let Some(parent) = it.path.parent() {
                            target = parent.to_path_buf();
                        }
                    }
                }
            }
            self.modal = Some(Modal::InputName { current: String::new(), target_dir: target });
            return Ok(false);
        }

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
                Focus::Content => Focus::Commits,
                Focus::Commits => Focus::Sidebar,
            };
            return Ok(false);
        }
        
        if !matches!(self.focus, Focus::Content) {
            match key.code {
                KeyCode::Up => {
                    match self.focus {
                        Focus::Sidebar => { self.handle_sidebar_key(key)?; return Ok(false); }
                        Focus::Commits => { self.git_section.select_prev(); return Ok(false); }
                        _ => {}
                    }
                }
                KeyCode::Down => {
                    match self.focus {
                        Focus::Sidebar => { self.handle_sidebar_key(key)?; return Ok(false); }
                        Focus::Commits => { self.git_section.select_next(); return Ok(false); }
                        _ => {}
                    }
                }
                KeyCode::Left => {
                    
                    if matches!(self.focus, Focus::Commits) || matches!(self.focus, Focus::Title) {
                        self.focus = Focus::Sidebar;
                        return Ok(false);
                    }
                }
                KeyCode::Right => {
                    
                    if matches!(self.focus, Focus::Sidebar) {
                        let sel = self.sidebar_state.selected().unwrap_or(0);
                        self.sidebar_enter_action(sel)?;
                        return Ok(false);
                    }
                    if matches!(self.focus, Focus::Commits) {
                        self.focus = match self.last_right_focus {
                            RightFocus::Title => Focus::Title,
                            RightFocus::Content => Focus::Content,
                        };
                        return Ok(false);
                    }
                }
                _ => {}
            }
        }

        match self.focus {
            Focus::Sidebar => self.handle_sidebar_key(key)?,
            Focus::Title => self.handle_title_key(key)?,
            Focus::Content => self.handle_content_key(key)?,
            Focus::Commits => self.handle_commits_key(key)?,
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
                self.sidebar_toggle_dir(selected)?;
            }
            KeyCode::Right => {
                self.sidebar_enter_action(selected)?;
            }
            KeyCode::Char('d') => {
                if selected < self.sidebar_items.len() {
                    let it = &self.sidebar_items[selected];
                    if !it.is_dir {
                        self.modal = Some(Modal::ConfirmDelete { path: it.path.clone() });
                    }
                }
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
        let was_expanded = self.expanded_dirs.contains(&item.path);
        if was_expanded {
            self.expanded_dirs.remove(&item.path);
            self.refresh_sidebar_preserve_selection(Some(idx));
        } else {
            self.expanded_dirs.insert(item.path.clone());
            self.refresh_sidebar_preserve_selection(Some(idx));
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

    fn handle_commits_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Up => {
                self.git_section.select_prev();
            }
            KeyCode::Down => {
                self.git_section.select_next();
            }
            KeyCode::Home => {
                if !self.git_section.commits.is_empty() {
                    self.git_section.selected = 0;
                }
            }
            KeyCode::End => {
                if !self.git_section.commits.is_empty() {
                    self.git_section.selected = self.git_section.commits.len() - 1;
                }
            }
            KeyCode::Left => {
                self.focus = Focus::Sidebar;
            }
            KeyCode::Right => {
                self.focus = match self.last_right_focus {
                    RightFocus::Title => Focus::Title,
                    RightFocus::Content => Focus::Content,
                };
            }
            KeyCode::Char('r') if key.modifiers.is_empty() => {
                self.git_section.fetch_and_refresh();
                self.status_message = Some("Fetched and refreshed commits".to_string());
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_modal_key(&mut self, key: KeyEvent) -> Result<()> {
        if let Some(modal) = &mut self.modal {
            match modal {
                Modal::ConfirmDelete { path } => {
                    match key.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') => {
                            if let Err(e) = std::fs::remove_file(path) {
                                self.status_message = Some(format!("Delete failed: {}", e));
                            } else {
                                self.status_message = Some("Deleted".to_string());
                                self.refresh_sidebar_preserve_selection(None);
                            }
                            self.modal = None;
                        }
                        KeyCode::Char('n') | KeyCode::Esc | KeyCode::Char('N') => {
                            self.modal = None;
                        }
                        _ => {}
                    }
                }
                Modal::InputName { current, target_dir } => {
                    match key.code {
                        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                            current.push(c);
                        }
                        KeyCode::Backspace => { current.pop(); }
                        KeyCode::Enter => {
                            if !current.trim().is_empty() {
                                self.title = current.trim().to_string();
                                self.title_cursor = self.title.len();
                                self.lines = vec![String::new()];
                                self.cursor_row = 0;
                                self.cursor_col = 0;
                                self.scroll_y = 0;
                                self.new_note_dir = Some(target_dir.clone());
                                self.opened_path = None;
                                self.dirty = true;
                                self.focus = Focus::Title;
                                self.last_right_focus = RightFocus::Title;
                                self.status_message = Some(format!("New note will be created in {}", target_dir.display()));
                            }
                            self.modal = None;
                        }
                        KeyCode::Esc => { self.modal = None; }
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }

    fn ensure_cursor_visible(&mut self) {
        let window = 20usize;
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

    

    fn save_current(&mut self) -> Result<()> {
        if self.title.trim().is_empty() {
            return Ok(());
        }
    let target_dir = self.new_note_dir.as_ref().unwrap_or(&self.notes_dir);
    let new_path = target_dir.join(format!("{}.md", self.title.trim()));
        let content = self.lines.join("\n");

        if let Some(old) = &self.opened_path {
            if *old != new_path {
                write_note(&new_path, &content)?;
                rename_note(old, &new_path).ok();
            } else {
                write_note(&new_path, &content)?;
            }
        } else {
            write_note(&new_path, &content)?;
        }

        self.opened_path = Some(new_path.clone());
        self.dirty = false;
    self.new_note_dir = None;

        
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
            ln.pop();
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
