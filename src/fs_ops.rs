
use anyhow::{Context, Result};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct NoteMeta {
    pub title: String,     // without .md
    pub path: PathBuf,     // full path to .md
}

pub fn ensure_notes_dir(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("Failed to create notes dir: {}", dir.display()))
}

pub fn list_notes(dir: &Path) -> Result<Vec<NoteMeta>> {
    let mut items: Vec<NoteMeta> = Vec::new();
    if !dir.exists() {
        ensure_notes_dir(dir)?;
    }
    for entry in fs::read_dir(dir).with_context(|| format!("Reading {}", dir.display()))? {
        let entry = entry?;
        let p = entry.path();
        if p.is_file() {
            if let Some(ext) = p.extension() {
                if ext == "md" {
                    if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                        items.push(NoteMeta {
                            title: stem.to_string(),
                            path: p.clone(),
                        });
                    }
                }
            }
        }
    }
    items.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    Ok(items)
}

pub fn read_note(path: &Path) -> Result<String> {
    let mut f = fs::File::open(path).with_context(|| format!("Open {}", path.display()))?;
    let mut s = String::new();
    f.read_to_string(&mut s)?;
    Ok(s)
}

pub fn write_note(path: &Path, content: &str) -> Result<()> {
    // ensure parent exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut f = fs::File::create(path).with_context(|| format!("Create {}", path.display()))?;
    f.write_all(content.as_bytes())?;
    Ok(())
}

pub fn rename_note(old: &Path, new: &Path) -> Result<()> {
    if old != new {
        fs::rename(old, new).with_context(|| format!("Rename {} -> {}", old.display(), new.display()))?;
    }
    Ok(())
}
