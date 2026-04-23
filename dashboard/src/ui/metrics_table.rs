use flux_proto::MetricSummary;
use ratatui::layout::{Alignment, Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Cell, Row, Table, TableState};
use ratatui::Frame;

use crate::app::{App, Liveness};
use crate::fmt;

const BORDER_ROWS: u16 = 2;
const HEADER_ROW: u16 = 1;
const NUMERIC_START_IDX: usize = 2;

const COLUMN_WIDTHS: [Constraint; 8] = [
    Constraint::Min(16),
    Constraint::Length(6),
    Constraint::Length(12),
    Constraint::Length(12),
    Constraint::Length(12),
    Constraint::Length(12),
    Constraint::Length(9),
    Constraint::Length(8),
];

const HEADERS: [&str; 8] = ["metric", "unit", "last", "min", "max", "avg", "rate", "age"];

const _: () = assert!(HEADERS.len() == COLUMN_WIDTHS.len());

pub fn render(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let visible = app.visible_metrics();
    let total = app.metrics.len();
    let shown = visible.len();

    let rows: Vec<Row<'static>> = visible.iter().map(|m| row_for(app, m)).collect();

    let header = Row::new(
        HEADERS
            .iter()
            .enumerate()
            .map(|(i, h)| header_cell(h, i))
            .collect::<Vec<_>>(),
    )
    .style(
        Style::default()
            .add_modifier(Modifier::BOLD)
            .fg(Color::Cyan),
    );

    let title = build_title(app, total, shown);
    let block = Block::default().borders(Borders::ALL).title(title);

    let body_rows = usize::from(area.height.saturating_sub(BORDER_ROWS + HEADER_ROW));
    let max_scroll = shown.saturating_sub(body_rows);
    let clamped_scroll = app.scroll.min(max_scroll);

    let mut state = TableState::default();
    *state.offset_mut() = clamped_scroll;

    let table = Table::new(rows, COLUMN_WIDTHS)
        .header(header)
        .block(block)
        .column_spacing(1);

    frame.render_stateful_widget(table, area, &mut state);
}

fn build_title(app: &App, total: usize, shown: usize) -> String {
    if app.filter.is_empty() {
        format!(" flux · {total} metric(s) ")
    } else {
        format!(" flux · {shown}/{total} · filter: {} ", app.filter)
    }
}

fn header_cell(label: &'static str, idx: usize) -> Cell<'static> {
    let line = Line::from(label);
    let aligned = if idx >= NUMERIC_START_IDX {
        line.alignment(Alignment::Right)
    } else {
        line.alignment(Alignment::Left)
    };
    Cell::from(aligned)
}

fn row_for(app: &App, m: &MetricSummary) -> Row<'static> {
    let age = app.age_ms(m);
    let liveness = app.liveness(age);

    let cells = vec![
        text_cell(m.name.clone(), Alignment::Left),
        text_cell(fmt::unit(m.unit.as_deref()).to_owned(), Alignment::Left),
        text_cell(fmt::value(m.last, m.kind), Alignment::Right),
        text_cell(fmt::value(m.min, m.kind), Alignment::Right),
        text_cell(fmt::value(m.max, m.kind), Alignment::Right),
        text_cell(fmt::value(m.avg, m.kind), Alignment::Right),
        text_cell(fmt::rate(m.rate_pps), Alignment::Right),
        text_cell(fmt::age(age), Alignment::Right).style(age_style(liveness)),
    ];
    Row::new(cells).style(row_style(liveness))
}

fn text_cell(text: String, align: Alignment) -> Cell<'static> {
    Cell::from(Line::from(text).alignment(align))
}

fn row_style(liveness: Liveness) -> Style {
    match liveness {
        Liveness::Fresh => Style::default(),
        Liveness::Stale | Liveness::Dead => {
            Style::default().fg(Color::Gray).add_modifier(Modifier::DIM)
        }
    }
}

fn age_style(liveness: Liveness) -> Style {
    match liveness {
        Liveness::Fresh => Style::default(),
        Liveness::Stale => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::DIM),
        Liveness::Dead => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
    }
}
