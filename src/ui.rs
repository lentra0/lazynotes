use crate::app::{App, Focus};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::*;
use ratatui::widgets::*;
use ratatui::text::{Line, Span};
use ratatui::style::{Style, Modifier, Color};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let size = frame.size();

    let outer_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(
            ratatui::widgets::block::Title::from("lazynotes")
                .alignment(Alignment::Center)
        )
        .title_style(Style::default().add_modifier(Modifier::BOLD));
    frame.render_widget(outer_block, size);
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .margin(1)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(size);

    let left_vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(20), Constraint::Percentage(20)])
        .split(chunks[0]);

    let middle_vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1), Constraint::Length(3)])
        .split(chunks[1]);

    draw_sidebar(frame, left_vertical[0], app);
    draw_changed_files(frame, left_vertical[1], app);
    draw_commit_list(frame, left_vertical[2], app);

    draw_right_panel(frame, middle_vertical[0], middle_vertical[1], app);
    draw_footer(frame, middle_vertical[2], app);
}

fn draw_sidebar(frame: &mut Frame, area: Rect, app: &mut App) {
    use ratatui::widgets::{List, ListItem, Block, Borders};

    let items: Vec<ListItem> = app
        .sidebar_items
        .iter()
        .map(|it| {
            let mut spans: Vec<Span> = Vec::new();
            
            if it.depth == 0 {
            } else {
                for (_level, anc_last) in it.last_ancestors.iter().enumerate() {
                    if *anc_last {
                        spans.push(Span::raw("  "));
                    } else {
                        spans.push(Span::raw("│ "));
                    }
                }
                
                let branch = if it.last_in_parent { "└─ " } else { "├─ " };
                spans.push(Span::raw(branch));
            }
            if it.is_dir {
                let icon = if it.expanded { "📂 " } else { "📁 " };
                spans.push(Span::styled(icon, Style::default().fg(Color::Yellow)));
                spans.push(Span::raw(format!("{}/", it.name)));
            } else {
                let icon = match it.path.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase()) {
                    Some(ext) if ["png", "jpg", "jpeg", "gif", "svg", "webp", "bmp"].contains(&ext.as_str()) => "🖼️ ",
                    _ => "📄 ",
                };
                spans.push(Span::raw(icon));
                spans.push(Span::raw(it.name.clone()));
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title("[1]Files")
        .title_style(Style::default().add_modifier(Modifier::BOLD))
        .border_style(if matches!(app.focus, Focus::Sidebar) { Style::default().fg(Color::Green).add_modifier(Modifier::BOLD) } else { Style::default() });

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD));

    frame.render_stateful_widget(list, area, &mut app.sidebar_state);

    
    if let Some(modal) = &app.modal {
        draw_modal(frame, modal, app);
    }
}

fn draw_modal(frame: &mut Frame, modal: &crate::app::Modal, _app: &App) {
    use ratatui::widgets::{Block, Borders, Paragraph};
    
    let area = frame.size();
    let w = (area.width as f32 * 0.5) as u16;
    let h = 7u16;
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let rect = Rect::new(x, y, w, h);

    let title = match modal {
        crate::app::Modal::ConfirmDelete { .. } => "Confirm Delete",
        crate::app::Modal::InputName { .. } => "New Note Name",
    };

    let block = Block::default().borders(Borders::ALL).title(title).border_type(ratatui::widgets::BorderType::Rounded);
    frame.render_widget(block, rect);

    let text = match modal {
        crate::app::Modal::ConfirmDelete { path } => vec![Line::from(Span::raw(format!("Delete {}? (y/n)", path.file_name().and_then(|s| s.to_str()).unwrap_or(""))))],
    crate::app::Modal::InputName { current, .. } => vec![Line::from(Span::raw(format!("Name: {}", current)))],
    };
    let para = Paragraph::new(Text::from(text)).alignment(Alignment::Left);
    let inner = Rect::new(rect.x + 1, rect.y + 1, rect.width.saturating_sub(2), rect.height.saturating_sub(2));
    frame.render_widget(para, inner);
}



fn draw_commit_list(frame: &mut Frame, area: Rect, app: &mut App) {
    use ratatui::widgets::{List, ListItem, Block, Borders};

    let commits = &app.git_section.commits;
    let selected = app.git_section.selected;
    let items: Vec<ListItem> = commits
        .iter()
        .map(|c| {
            let summary = format!("{} {}", &c.hash, &c.summary);
            let line1 = Line::from(Span::raw(summary));
            let line2 = Line::from(Span::styled(format!("{} • {}", &c.author, &c.date), Style::default().add_modifier(Modifier::ITALIC)));
            ListItem::new(vec![line1, line2])
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title("[4]Recent Commits")
        .title_style(Style::default().add_modifier(Modifier::BOLD))
        .title_alignment(Alignment::Left)
        .border_style(if matches!(app.focus, Focus::Commits) {
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        });

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD))
        .highlight_symbol("→ ");

    let mut state = ratatui::widgets::ListState::default();
    if !commits.is_empty() {
        state.select(Some(selected));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_changed_files(frame: &mut Frame, area: Rect, app: &mut App) {
    use ratatui::widgets::{Paragraph, Block, Borders};
    let files = app.git_section.selected_changed_files();
    let file_items: Vec<Line> = if files.is_empty() {
        vec![Line::raw("(no changed files)")]
    } else {
        files.iter().map(|f| Line::from(Span::raw(f.clone()))).collect()
    };
    let files_para = Paragraph::new(Text::from(file_items))
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title("Changed Files"));
    frame.render_widget(files_para, area);
}

fn draw_right_panel(frame: &mut Frame, title_area: Rect, content_area: Rect, app: &mut App) {
    let title_style = if matches!(app.focus, Focus::Title) {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let title = Paragraph::new(app.title.as_str())
        .block(
                Block::default()
                .title(
                    ratatui::widgets::block::Title::from("[2]Title")
                        .alignment(Alignment::Left)
                )
                .title_style(title_style)
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(if matches!(app.focus, Focus::Title) { Style::default().fg(Color::Green).add_modifier(Modifier::BOLD) } else { Style::default() }),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(title, title_area);

    let text_lines: Vec<Line> = if app.lines.is_empty() {
        vec![Line::raw("")]
    } else {
        app.lines.iter().map(|l| Line::raw(l.clone())).collect()
    };

    let paragraph = Paragraph::new(Text::from(text_lines))
        .block(
                Block::default()
                .title(
                    ratatui::widgets::block::Title::from(if app.dirty { "[3]Content *" } else { "[3]Content" })
                        .alignment(Alignment::Left)
                )
                .title_style(Style::default().add_modifier(Modifier::BOLD))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(if matches!(app.focus, Focus::Content) { Style::default().fg(Color::Green).add_modifier(Modifier::BOLD) } else { Style::default() }),
        )
        .wrap(Wrap { trim: false })
        .scroll((app.scroll_y as u16, 0));
    frame.render_widget(paragraph, content_area);

    match app.focus {
        Focus::Title => {
            let x = title_area.x + 1 + app.title_cursor as u16;
            let y = title_area.y + 1;
            frame.set_cursor(x.min(title_area.right().saturating_sub(2)), y);
        }
        Focus::Content => {
            let (cx, cy) = content_cursor_to_screen(content_area, app);
            frame.set_cursor(cx, cy);
        }
        _ => {}
    }
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &mut App) {
    
    let help = Line::from(vec![
    Span::styled("Ctrl+S", Style::default().fg(Color::LightMagenta)), Span::raw(":Save"), Span::raw("  "),
    Span::styled("Enter/Right", Style::default().fg(Color::Green)), Span::raw(":Open"), Span::raw("  "),
    Span::styled("d", Style::default().fg(Color::LightRed)), Span::raw(":Delete"), Span::raw("  "),
    
    ]);
    
    let mut footer_text = vec![help];
    if let Some(msg) = &app.status_message {
        footer_text.push(Line::from(Span::raw(format!("  {}", msg))));
    }

    let footer = Paragraph::new(Text::from(footer_text))
        .alignment(Alignment::Center)
    .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded));
    frame.render_widget(footer, area);
}

fn content_cursor_to_screen(area: Rect, app: &App) -> (u16, u16) {
    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    let visible_row = app.cursor_row.saturating_sub(app.scroll_y);
    let y = inner.y + (visible_row as u16).min(inner.height.saturating_sub(1));
    let x = inner.x + (app.cursor_col as u16).min(inner.width.saturating_sub(1));
    (x, y)
}
