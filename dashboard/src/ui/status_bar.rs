use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, ConnectionState};
use crate::fmt;

pub fn render_status(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let (symbol, text, color) = status_text(app);
    let mut spans = vec![
        Span::styled(
            format!(" {symbol} "),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::raw(text),
    ];
    spans.extend(hint_spans(app.filter_mode));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

pub fn render_filter(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let (prefix, caret_color) = if app.filter_mode {
        ("filter › ", Color::Yellow)
    } else {
        ("filter: ", Color::Gray)
    };
    let mut spans = vec![
        Span::styled(
            prefix,
            Style::default()
                .fg(caret_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(app.filter.clone()),
    ];
    if app.filter_mode {
        spans.push(Span::styled(
            "_",
            Style::default()
                .fg(caret_color)
                .add_modifier(Modifier::SLOW_BLINK),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn status_text(app: &App) -> (&'static str, String, Color) {
    match &app.connection {
        ConnectionState::Connecting => {
            ("◐", format!("connecting {}", app.server_url), Color::Yellow)
        }
        ConnectionState::Connected => {
            let age_ms = app.last_snapshot_at.map_or(0, |t| {
                u64::try_from(t.elapsed().as_millis()).unwrap_or(u64::MAX)
            });
            (
                "●",
                format!(
                    "connected {} · snapshot {} ago",
                    app.server_url,
                    fmt::age(Some(age_ms))
                ),
                Color::Green,
            )
        }
        ConnectionState::Reconnecting { attempt, in_ms } => (
            "◐",
            format!(
                "reconnecting {} · try {attempt}, next in {}",
                app.server_url,
                fmt::countdown_secs(*in_ms),
            ),
            Color::Yellow,
        ),
        ConnectionState::Disconnected => {
            ("✕", format!("disconnected {}", app.server_url), Color::Red)
        }
    }
}

fn hint_spans(filter_mode: bool) -> Vec<Span<'static>> {
    let key_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    if filter_mode {
        vec![
            Span::raw("   "),
            Span::styled("Enter", key_style),
            Span::raw(" apply · "),
            Span::styled("Esc", key_style),
            Span::raw(" cancel"),
        ]
    } else {
        vec![
            Span::raw("   "),
            Span::styled("?", key_style),
            Span::raw(" help · "),
            Span::styled("/", key_style),
            Span::raw(" filter · "),
            Span::styled("q", key_style),
            Span::raw(" quit"),
        ]
    }
}
