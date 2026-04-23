use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{App, ConnectionState};
use crate::fmt;

const SEPARATOR: &str = "  │  ";

pub fn render(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let h = &app.health;
    let has_errors = h.packets_parse_err > 0;
    let fields: [(&str, String, Style); 5] = [
        ("pps", fmt::rate_bare(h.ingest_rate_pps), value_style()),
        (
            "subs",
            fmt::integer_with_separators(u64::from(h.subscriber_count)),
            value_style(),
        ),
        ("up", fmt::uptime(h.uptime_ms), value_style()),
        (
            "pkts",
            fmt::integer_with_separators(h.packets_received),
            value_style(),
        ),
        (
            "parse-err",
            fmt::integer_with_separators(h.packets_parse_err),
            if has_errors {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                value_style()
            },
        ),
    ];

    let mut spans: Vec<Span<'static>> = Vec::with_capacity(fields.len() * 3);
    for (i, (label, value, value_style)) in fields.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(
                SEPARATOR,
                Style::default().fg(Color::DarkGray),
            ));
        }
        spans.push(Span::styled(
            format!("{label} "),
            Style::default().fg(Color::DarkGray),
        ));
        spans.push(Span::styled(value.clone(), *value_style));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color(app)))
        .title(Span::styled(
            " pipeline health ",
            Style::default().fg(Color::Gray),
        ));
    let paragraph = Paragraph::new(Line::from(spans)).block(block);
    frame.render_widget(paragraph, area);
}

fn value_style() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}

fn border_color(app: &App) -> Color {
    match app.connection {
        ConnectionState::Connected => Color::DarkGray,
        ConnectionState::Connecting | ConnectionState::Reconnecting { .. } => Color::Yellow,
        ConnectionState::Disconnected => Color::Red,
    }
}
