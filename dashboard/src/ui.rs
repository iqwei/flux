pub mod health_panel;
pub mod help;
pub mod layout;
pub mod metrics_table;
pub mod status_bar;

use ratatui::Frame;

use crate::app::App;

pub fn render(frame: &mut Frame<'_>, app: &App) {
    let full = frame.area();
    let areas = layout::Areas::compute(full, app);
    metrics_table::render(frame, app, areas.table);
    health_panel::render(frame, app, areas.health);
    if let Some(filter_area) = areas.filter {
        status_bar::render_filter(frame, app, filter_area);
    }
    status_bar::render_status(frame, app, areas.status);
    if app.help_open {
        help::render(frame, full);
    }
}
