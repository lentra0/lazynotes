use anyhow::Result;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub hash: String,
    pub summary: String,
    pub author: String,
    pub date: String,
    pub changed_files: Vec<String>,
}

pub fn get_recent_commits(limit: usize, path: Option<&Path>) -> Result<Vec<CommitInfo>> {
    use std::process::Command;
    let mut cmd = Command::new("git");
    if let Some(p) = path {
        cmd.current_dir(p);
    }
    cmd.arg("log").arg(format!("-n{}", limit)).arg("--pretty=format:%h|%s|%an|%ar");
    let output = cmd.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut commits: Vec<CommitInfo> = stdout
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(4, '|');
            Some(CommitInfo {
                hash: parts.next()?.to_string(),
                summary: parts.next()?.to_string(),
                author: parts.next()?.to_string(),
                date: parts.next()?.to_string(),
                changed_files: Vec::new(),
            })
        })
        .collect();

    for c in &mut commits {
        let mut show_cmd = Command::new("git");
        if let Some(p) = path {
            show_cmd.current_dir(p);
        }
        show_cmd.arg("diff-tree").arg("--no-commit-id").arg("--name-only").arg("-r").arg(&c.hash);
        if let Ok(out) = show_cmd.output() {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout);
                c.changed_files = s.lines().map(|l| l.to_string()).filter(|l| !l.is_empty()).collect();
            }
        }
    }

    Ok(commits)
}

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
