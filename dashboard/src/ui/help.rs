use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

const POPUP_WIDTH: u16 = 46;
const POPUP_HEIGHT: u16 = 14;

pub fn render(frame: &mut Frame<'_>, area: Rect) {
    let popup = centered(area, POPUP_WIDTH, POPUP_HEIGHT);
    frame.render_widget(Clear, popup);

    let lines = vec![
        section("navigation"),
        binding("↑ / k", "scroll up"),
        binding("↓ / j", "scroll down"),
        binding("PgUp / PgDn", "scroll by page"),
        binding("Home / End", "jump to top / bottom"),
        Line::from(""),
        section("filter"),
        binding("/", "enter filter (substring, case-insensitive)"),
        binding("Esc", "clear filter"),
        Line::from(""),
        section("app"),
        binding("?", "toggle this help"),
        binding("q / Ctrl-C", "quit"),
    ];

    let title = Line::from(vec![
        Span::styled(
            " keys ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " press ? or Esc to close ",
            Style::default().fg(Color::Gray),
        ),
    ]);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(title);
    let paragraph = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Left);
    frame.render_widget(paragraph, popup);
}

fn section(label: &'static str) -> Line<'static> {
    Line::from(Span::styled(
        format!("  {label}"),
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    ))
}

fn binding(keys: &'static str, description: &'static str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("    {keys:<12}"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(description, Style::default().fg(Color::Gray)),
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
