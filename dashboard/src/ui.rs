use flux_proto::{MetricKind, MetricSummary};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use ratatui::Frame;

use crate::app::{App, ConnectionState};

const STATUS_BAR_HEIGHT: u16 = 1;

const COLUMN_WIDTHS: [Constraint; 8] = [
    Constraint::Min(16),
    Constraint::Length(8),
    Constraint::Length(12),
    Constraint::Length(12),
    Constraint::Length(12),
    Constraint::Length(12),
    Constraint::Length(8),
    Constraint::Length(10),
];

const HEADER_LABELS: [&str; 8] = [
    "metric", "unit", "last", "min", "max", "avg", "pps", "samples",
];

const _: () = assert!(
    HEADER_LABELS.len() == COLUMN_WIDTHS.len(),
    "HEADER_LABELS and COLUMN_WIDTHS must stay the same length",
);

pub fn render(frame: &mut Frame<'_>, app: &App) {
    let [table_area, status_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(STATUS_BAR_HEIGHT)])
            .areas(frame.area());

    render_metrics_table(frame, app, table_area);
    render_status_bar(frame, app, status_area);
}

fn render_metrics_table(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let header = Row::new(
        HEADER_LABELS
            .iter()
            .copied()
            .map(Cell::from)
            .collect::<Vec<_>>(),
    )
    .style(Style::default().add_modifier(Modifier::BOLD));

    let rows: Vec<Row<'static>> = app.metrics.iter().map(metric_row).collect();

    let title = format!(
        " flux · {} · {} metric(s) ",
        app.server_url,
        app.metrics.len()
    );
    let block = Block::default().borders(Borders::ALL).title(title);

    let table = Table::new(rows, COLUMN_WIDTHS)
        .header(header)
        .block(block)
        .column_spacing(1);

    frame.render_widget(table, area);
}

fn metric_row(m: &MetricSummary) -> Row<'static> {
    Row::new(vec![
        Cell::from(m.name.clone()),
        Cell::from(m.unit.clone().unwrap_or_default()),
        Cell::from(format_value(m.last, m.kind)),
        Cell::from(format_value(m.min, m.kind)),
        Cell::from(format_value(m.max, m.kind)),
        Cell::from(format_value(m.avg, m.kind)),
        Cell::from(format_rate(m.rate_pps)),
        Cell::from(m.sample_count.to_string()),
    ])
}

fn format_value(v: Option<f64>, kind: MetricKind) -> String {
    match v {
        None => "—".to_owned(),
        Some(x) if !x.is_finite() => "—".to_owned(),
        Some(x) => match kind {
            MetricKind::U64 | MetricKind::I64 => format!("{x:.0}"),
            MetricKind::F64 => format!("{x:.3}"),
            MetricKind::Bool => {
                if x == 0.0 {
                    "false".to_owned()
                } else {
                    "true".to_owned()
                }
            }
        },
    }
}

fn format_rate(pps: f64) -> String {
    if pps.is_finite() {
        format!("{pps:.1}")
    } else {
        "—".to_owned()
    }
}

fn render_status_bar(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let (symbol, text, color) = match &app.connection {
        ConnectionState::Connecting => {
            ("◐", format!("connecting {}", app.server_url), Color::Yellow)
        }
        ConnectionState::Connected => {
            let age_ms = app.last_snapshot_at.map_or(0u64, |t| {
                u64::try_from(t.elapsed().as_millis()).unwrap_or(u64::MAX)
            });
            (
                "●",
                format!(
                    "connected {} · last snapshot {}ms ago",
                    app.server_url, age_ms
                ),
                Color::Green,
            )
        }
        ConnectionState::Reconnecting { attempt, in_ms } => (
            "◐",
            format!(
                "reconnecting {} (try {attempt}, next in {in_ms}ms)",
                app.server_url
            ),
            Color::Yellow,
        ),
        ConnectionState::Disconnected => {
            ("✕", format!("disconnected {}", app.server_url), Color::Red)
        }
    };

    let line = Line::from(vec![
        Span::styled(
            format!(" {symbol} "),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::raw(text),
    ]);

    frame.render_widget(Paragraph::new(line), area);
}
