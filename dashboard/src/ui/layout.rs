use ratatui::layout::{Constraint, Layout, Rect};

use crate::app::App;

const HEALTH_ROWS: u16 = 3;
const STATUS_ROWS: u16 = 1;
const FILTER_ROWS: u16 = 1;

#[derive(Debug, Clone, Copy)]
pub struct Areas {
    pub table: Rect,
    pub health: Rect,
    pub filter: Option<Rect>,
    pub status: Rect,
}

impl Areas {
    #[must_use]
    pub fn compute(area: Rect, app: &App) -> Self {
        let show_filter = app.filter_mode || !app.filter.is_empty();
        if show_filter {
            let [table, health, filter, status] = Layout::vertical([
                Constraint::Min(1),
                Constraint::Length(HEALTH_ROWS),
                Constraint::Length(FILTER_ROWS),
                Constraint::Length(STATUS_ROWS),
            ])
            .areas(area);
            Self {
                table,
                health,
                filter: Some(filter),
                status,
            }
        } else {
            let [table, health, status] = Layout::vertical([
                Constraint::Min(1),
                Constraint::Length(HEALTH_ROWS),
                Constraint::Length(STATUS_ROWS),
            ])
            .areas(area);
            Self {
                table,
                health,
                filter: None,
                status,
            }
        }
    }
}
