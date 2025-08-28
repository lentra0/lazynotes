use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub enum NoteNode {
    Dir {
        name: String,
        path: PathBuf,
        children: Vec<NoteNode>,
    },
    File {
        title: String, // stem without .md
        path: PathBuf, // full path to .md
    },
}

#[derive(Debug, Clone)]
pub struct FlatNode {
    pub name: String,       // name displayed (folder or file stem)
    pub depth: usize,       // depth level for indentation
    pub path: PathBuf,      // dir or file path
    pub is_dir: bool,       // true for folder, false for file
    pub expanded: bool,     // only meaningful for dir
}

pub fn ensure_notes_dir(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("Failed to create notes dir: {}", dir.display()))
}

pub fn read_note(path: &Path) -> Result<String> {
    let mut f = fs::File::open(path).with_context(|| format!("Open {}", path.display()))?;
    let mut s = String::new();
    f.read_to_string(&mut s)?;
    Ok(s)
}

pub fn write_note(path: &Path, content: &str) -> Result<()> {
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

pub fn build_notes_tree(dir: &Path) -> Result<NoteNode> {
    let mut children: Vec<NoteNode> = Vec::new();

    if !dir.exists() {
        ensure_notes_dir(dir)?;
    }

    for entry in fs::read_dir(dir).with_context(|| format!("Reading {}", dir.display()))? {
        let entry = entry?;
        let p = entry.path();

        if p.is_dir() {
            // skip hidden directories by default (feel free to remove this if you want dotfolders)
            if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                if name.starts_with('.') {
                    continue;
                }
            }
            children.push(build_notes_tree(&p)?);
        } else if p.is_file() {
            if p.extension().and_then(|e| e.to_str()) == Some("md") {
                if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                    children.push(NoteNode::File {
                        title: stem.to_string(),
                        path: p.clone(),
                    });
                }
            }
        }
    }

    // sort: dirs first (by name), then files (by title), case-insensitive
    children.sort_by(|a, b| match (a, b) {
        (NoteNode::Dir { name: an, .. }, NoteNode::Dir { name: bn, .. }) => an.to_lowercase().cmp(&bn.to_lowercase()),
        (NoteNode::Dir { .. }, NoteNode::File { .. }) => std::cmp::Ordering::Less,
        (NoteNode::File { .. }, NoteNode::Dir { .. }) => std::cmp::Ordering::Greater,
        (NoteNode::File { title: an, .. }, NoteNode::File { title: bn, .. }) => an.to_lowercase().cmp(&bn.to_lowercase()),
    });

    let name = dir
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| dir.to_string_lossy().to_string());


    Ok(NoteNode::Dir {
        name,
        path: dir.to_path_buf(),
        children,
    })
}

// Flatten the tree for display as a 1D list with indentation.
// We skip the root node row and render only its children at depth 0.
pub fn flatten_tree_for_sidebar(root: &NoteNode, expanded: &HashSet<PathBuf>) -> Vec<FlatNode> {
    let mut out = Vec::new();
    match root {
        NoteNode::Dir { children, .. } => {
            for child in children {
                flatten_node(child, 0, expanded, &mut out);
            }
        }
        NoteNode::File { .. } => {
            // Shouldn't happen for our root, but handle anyway
            flatten_node(root, 0, expanded, &mut out);
        }
    }
    out
}

fn flatten_node(node: &NoteNode, depth: usize, expanded: &HashSet<PathBuf>, out: &mut Vec<FlatNode>) {
    match node {
        NoteNode::Dir { name, path, children } => {
            let is_expanded = expanded.contains(path);
            out.push(FlatNode {
                name: name.clone(),
                depth,
                path: path.clone(),
                is_dir: true,
                expanded: is_expanded,
            });
            if is_expanded {
                for child in children {
                    flatten_node(child, depth + 1, expanded, out);
                }
            }
        }
        NoteNode::File { title, path } => {
            out.push(FlatNode {
                name: title.clone(),
                depth,
                path: path.clone(),
                is_dir: false,
                expanded: false,
            });
        }
    }
}
