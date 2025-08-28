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
        title: String,
        path: PathBuf,
    },
}

#[derive(Debug, Clone)]
pub struct FlatNode {
    pub name: String,
    pub depth: usize,
    pub path: PathBuf,
    pub is_dir: bool,
    pub expanded: bool,
    pub last_in_parent: bool,
    pub last_ancestors: Vec<bool>,
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
            if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                if name.starts_with('.') {
                    continue;
                }
            }
            children.push(build_notes_tree(&p)?);
        } else if p.is_file() {
            if let Some(fname) = p.file_name().and_then(|s| s.to_str()) {
                if fname.starts_with('.') {
                } else {
                    children.push(NoteNode::File {
                        title: fname.to_string(),
                        path: p.clone(),
                    });
                }
            }
        }
    }

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

pub fn flatten_tree_for_sidebar(root: &NoteNode, expanded: &HashSet<PathBuf>) -> Vec<FlatNode> {
    let mut out = Vec::new();
    match root {
        NoteNode::Dir { children, .. } => {
            for (i, child) in children.iter().enumerate() {
                let is_last = i + 1 == children.len();
                flatten_node(child, 0, expanded, is_last, &mut out, &mut Vec::new());
            }
        }
        NoteNode::File { .. } => {
            flatten_node(root, 0, expanded, true, &mut out, &mut Vec::new());
        }
    }
    out
}

fn flatten_node(
    node: &NoteNode,
    depth: usize,
    expanded: &HashSet<PathBuf>,
    last_in_parent: bool,
    out: &mut Vec<FlatNode>,
    ancestors_last: &mut Vec<bool>,
) {
    match node {
        NoteNode::Dir { name, path, children } => {
            let is_expanded = expanded.contains(path);
            out.push(FlatNode {
                name: name.clone(),
                depth,
                path: path.clone(),
                is_dir: true,
                expanded: is_expanded,
                last_in_parent,
                last_ancestors: ancestors_last.clone(),
            });
            if is_expanded {
                ancestors_last.push(last_in_parent);
                for (i, child) in children.iter().enumerate() {
                    let child_is_last = i + 1 == children.len();
                    flatten_node(child, depth + 1, expanded, child_is_last, out, ancestors_last);
                }
                ancestors_last.pop();
            }
        }
        NoteNode::File { title, path } => {
            out.push(FlatNode {
                name: title.clone(),
                depth,
                path: path.clone(),
                is_dir: false,
                expanded: false,
                last_in_parent,
                last_ancestors: ancestors_last.clone(),
            });
        }
    }
}
