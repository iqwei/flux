use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::fmt;

const SEPARATOR: &str = "  │  ";

pub fn render(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let h = &app.health;
    let fields: [(&str, String); 5] = [
        ("pps", format!("{:.1}", h.ingest_rate_pps)),
        ("subs", h.subscriber_count.to_string()),
        ("up", fmt::uptime(h.uptime_ms)),
        ("pkts", h.packets_received.to_string()),
        ("parse-err", h.packets_parse_err.to_string()),
    ];

    let mut spans: Vec<Span<'static>> = Vec::with_capacity(fields.len() * 3 + fields.len());
    for (i, (label, value)) in fields.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(
                SEPARATOR,
                Style::default().fg(Color::DarkGray),
            ));
        }
        spans.push(Span::styled(
            format!("{label} "),
            Style::default().fg(Color::Gray),
        ));
        spans.push(Span::styled(
            value.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        ));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" pipeline health ");
    let paragraph = Paragraph::new(Line::from(spans)).block(block);
    frame.render_widget(paragraph, area);
}
