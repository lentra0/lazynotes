use crate::app::{App, Focus};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::*;
use ratatui::widgets::*;
use ratatui::text::{Line, Span};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let size = frame.size();

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(size);

    draw_sidebar(frame, chunks[0], app);
    draw_right_panel(frame, chunks[1], app);
}

fn draw_sidebar(frame: &mut Frame, area: Rect, app: &mut App) {
    let items: Vec<ListItem> = app
        .sidebar_items
        .iter()
        .map(|n| {
            let indent = "  ".repeat(n.depth);
            let marker = if n.is_dir {
                if n.expanded { "▾ " } else { "▸ " }
            } else {
                "  "
            };
            let display = format!("{}{}{}", indent, marker, n.name);
            ListItem::new(Line::from(display))
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Notes ")
        .border_style(if matches!(app.focus, Focus::Sidebar) {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        });

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("▪ ");

    frame.render_stateful_widget(list, area, &mut app.sidebar_state);
}

fn draw_right_panel(frame: &mut Frame, area: Rect, app: &mut App) {
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    // Title editor
    let title_style = if matches!(app.focus, Focus::Title) {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    let title = Paragraph::new(app.title.as_str())
        .block(
            Block::default()
                .title(" Title (without .md) ")
                .borders(Borders::ALL)
                .border_style(title_style),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(title, right_chunks[0]);

    // Content editor
    let cont_style = if matches!(app.focus, Focus::Content) {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };

    let text_lines: Vec<Line> = if app.lines.is_empty() {
        vec![Line::raw("")]
    } else {
        app.lines.iter().map(|l| Line::raw(l.clone())).collect()
    };

    let paragraph = Paragraph::new(Text::from(text_lines))
        .block(
            Block::default()
                .title(if app.dirty { " Content * " } else { " Content " })
                .borders(Borders::ALL)
                .border_style(cont_style),
        )
        .wrap(Wrap { trim: false })
        .scroll((app.scroll_y as u16, 0));
    frame.render_widget(paragraph, right_chunks[1]);

    // Cursor placement
    match app.focus {
        Focus::Title => {
            let x = right_chunks[0].x + 1 + app.title_cursor as u16;
            let y = right_chunks[0].y + 1;
            frame.set_cursor(x.min(right_chunks[0].right().saturating_sub(2)), y);
        }
        Focus::Content => {
            let (cx, cy) = content_cursor_to_screen(right_chunks[1], app);
            frame.set_cursor(cx, cy);
        }
        _ => {}
    }

    // Footer help
    let help = Line::from(vec![
        Span::styled("Tab", Style::default().fg(Color::Yellow)),
        Span::raw(" cycle  "),
        Span::styled("Space/Enter", Style::default().fg(Color::Yellow)),
        Span::raw(" expand  "),
        Span::styled("n", Style::default().fg(Color::Yellow)),
        Span::raw(" new  "),
        Span::styled("Ctrl+S", Style::default().fg(Color::Yellow)),
        Span::raw(" save  "),
        Span::styled("q", Style::default().fg(Color::Yellow)),
        Span::raw(" quit"),
    ]);
    let footer = Paragraph::new(help).alignment(Alignment::Center);
    frame.render_widget(footer, right_chunks[2]);
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
