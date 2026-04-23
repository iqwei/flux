use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

const POPUP_WIDTH: u16 = 42;
const POPUP_HEIGHT: u16 = 12;

pub fn render(frame: &mut Frame<'_>, area: Rect) {
    let popup = centered(area, POPUP_WIDTH, POPUP_HEIGHT);
    frame.render_widget(Clear, popup);

    let lines = vec![
        binding("↑ / k", "scroll up"),
        binding("↓ / j", "scroll down"),
        binding("PgUp / PgDn", "scroll by page"),
        binding("Home / End", "jump to top / bottom"),
        binding("/", "enter filter"),
        binding("Esc", "clear filter"),
        binding("?", "toggle this help"),
        binding("q / Ctrl-C", "quit"),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" keys · ? to close ");
    let paragraph = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Left);
    frame.render_widget(paragraph, popup);
}

fn binding(keys: &'static str, description: &'static str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {keys:<14}"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(description),
    ])
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    Rect {
        x,
        y,
        width: w,
        height: h,
    }
}
