use crate::git::{CommitInfo, get_recent_commits};
use std::path::PathBuf;

pub struct GitSection {
    pub commits: Vec<CommitInfo>,
    pub selected: usize,
    pub path: Option<PathBuf>,
}

impl GitSection {
    pub fn new_for(path: Option<PathBuf>) -> Self {
        let commits = get_recent_commits(30, path.as_deref()).unwrap_or_default();
        Self { commits, selected: 0, path }
    }

    pub fn refresh(&mut self) {
        self.commits = get_recent_commits(30, self.path.as_deref()).unwrap_or_default();
        self.selected = 0;
    }

    pub fn fetch_and_refresh(&mut self) {
        use std::process::Command;
        if let Some(p) = &self.path {
            let _ = Command::new("git").arg("-C").arg(p).arg("fetch").output();
        } else {
            let _ = Command::new("git").arg("fetch").output();
        }
        self.refresh();
    }

    pub fn selected_changed_files(&self) -> Vec<String> {
        if self.commits.is_empty() { return Vec::new(); }
        self.commits.get(self.selected).map(|c| c.changed_files.clone()).unwrap_or_default()
    }

    pub fn select_next(&mut self) {
        if !self.commits.is_empty() {
            self.selected = (self.selected + 1).min(self.commits.len() - 1);
        }
    }
    pub fn select_prev(&mut self) {
        if !self.commits.is_empty() {
            self.selected = self.selected.saturating_sub(1);
        }
    }
}
