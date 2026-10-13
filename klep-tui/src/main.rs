//! klepci TUI: minimalist terminal frontend over [`klep_core::Core`].
//!
//! Keys: `1/2/3` tabs (decks/review/stats), `↑↓` move, `Enter` select,
//! `space` show answer, `1-4` rate, `q` quit.

use std::io;
use std::path::PathBuf;
use std::time::Instant;

use crossterm::event::Event;
use crossterm::event::KeyCode;
use crossterm::event::KeyEventKind;
use crossterm::execute;
use crossterm::terminal::disable_raw_mode;
use crossterm::terminal::enable_raw_mode;
use crossterm::terminal::EnterAlternateScreen;
use crossterm::terminal::LeaveAlternateScreen;
use klep_core::AnswerView;
use klep_core::CardView;
use klep_core::Core;
use klep_core::DeckInfo;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::Borders;
use ratatui::widgets::List;
use ratatui::widgets::ListItem;
use ratatui::widgets::ListState;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Wrap;
use ratatui::Terminal;

fn default_collection_path() -> PathBuf {
    let base = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let dir = PathBuf::from(format!("{base}/.local/share/klepci"));
    let _ = std::fs::create_dir_all(&dir);
    dir.join("collection.anki2")
}

fn html_to_text(html: &str) -> String {
    let mut out = html
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("</p>", "\n")
        .replace("</div>", "\n")
        .replace("</li>", "\n")
        .replace("<li>", "\n• ");
    while let Some(start) = out.find("[sound:") {
        let Some(end) = out[start..].find(']') else {
            break;
        };
        out.replace_range(start..start + end + 1, "🔊");
    }
    while let Some(start) = out.find("<img") {
        let Some(end) = out[start..].find('>') else {
            break;
        };
        out.replace_range(start..start + end + 1, "[image]");
    }
    let mut text = String::with_capacity(out.len());
    let mut inside = false;
    for ch in out.chars() {
        match ch {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => text.push(ch),
            _ => {}
        }
    }
    text.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Tab {
    Decks,
    Review,
    Stats,
}

struct App {
    core: Option<Core>,
    error: Option<String>,
    tab: Tab,
    decks: Vec<DeckInfo>,
    deck_state: ListState,
    card: Option<CardView>,
    answer: Option<AnswerView>,
    shown_at: Option<Instant>,
    done: bool,
    stats: Option<serde_json::Value>,
}

impl App {
    fn new() -> Self {
        let mut app = App {
            core: None,
            error: None,
            tab: Tab::Decks,
            decks: vec![],
            deck_state: ListState::default(),
            card: None,
            answer: None,
            shown_at: None,
            done: false,
            stats: None,
        };
        match Core::open(&default_collection_path()) {
            Ok(core) => {
                app.core = Some(core);
                app.refresh_decks();
            }
            Err(err) => app.error = Some(err.to_string()),
        }
        if !app.decks.is_empty() {
            app.deck_state.select(Some(0));
        }
        app
    }

    fn fail<T>(&mut self, result: Result<T, klep_core::CoreError>) -> Option<T> {
        match result {
            Ok(value) => {
                self.error = None;
                Some(value)
            }
            Err(err) => {
                self.error = Some(err.to_string());
                None
            }
        }
    }

    fn refresh_decks(&mut self) {
        let decks = self.core.as_ref().map(|core| core.decks());
        if let Some(result) = decks {
            if let Some(decks) = self.fail(result) {
                self.decks = decks;
            }
        }
    }

    fn next(&mut self) {
        self.card = None;
        self.answer = None;
        self.shown_at = None;
        self.done = false;
        let next = self.core.as_mut().map(|core| core.next_card());
        if let Some(result) = next {
            match self.fail(result) {
                Some(Some(card)) => {
                    self.card = Some(card);
                    self.shown_at = Some(Instant::now());
                }
                Some(None) => self.done = true,
                None => {}
            }
        }
    }

    fn show(&mut self) {
        let card_id = match &self.card {
            Some(card) => card.id,
            None => return,
        };
        let rendered = self.core.as_mut().map(|core| core.show_answer(card_id));
        if let Some(result) = rendered {
            if let Some(answer) = self.fail(result) {
                self.answer = Some(answer);
            }
        }
    }

    fn answer(&mut self, rating: u8) {
        let card_id = match &self.card {
            Some(card) => card.id,
            None => return,
        };
        let secs = self
            .shown_at
            .map_or(1, |at| at.elapsed().as_secs().max(1) as u32);
        let result = self
            .core
            .as_mut()
            .map(|core| core.answer(card_id, rating, secs));
        if let Some(result) = result {
            if self.fail(result).is_some() {
                self.next();
            }
        }
    }

    fn load_stats(&mut self) {
        let json = self.core.as_mut().map(|core| core.timing_json("", 10));
        self.stats = match json {
            Some(result) => self
                .fail(result)
                .and_then(|text| serde_json::from_str(&text).ok()),
            None => None,
        };
    }
}

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut app = App::new();

    loop {
        terminal.draw(|frame| render(&mut app, frame))?;
        if let Event::Key(key) = crossterm::event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Char('1') => {
                    app.tab = Tab::Decks;
                }
                KeyCode::Char('2') => {
                    app.tab = Tab::Review;
                    if app.card.is_none() {
                        app.next();
                    }
                }
                KeyCode::Char('3') => {
                    app.tab = Tab::Stats;
                    app.load_stats();
                }
                KeyCode::Tab => {
                    app.tab = match app.tab {
                        Tab::Decks => Tab::Review,
                        Tab::Review => Tab::Stats,
                        Tab::Stats => Tab::Decks,
                    };
                    if app.tab == Tab::Review && app.card.is_none() {
                        app.next();
                    }
                    if app.tab == Tab::Stats {
                        app.load_stats();
                    }
                }
                KeyCode::Up => move_selection(&mut app, -1),
                KeyCode::Down => move_selection(&mut app, 1),
                KeyCode::Enter => {
                    if app.tab == Tab::Decks {
                        app.tab = Tab::Review;
                        app.next();
                    } else if app.answer.is_none() {
                        app.show();
                    }
                }
                KeyCode::Char(' ') if app.tab == Tab::Review => app.show(),
                _ => {}
            }
            // number keys double as ratings once the answer is visible
            if app.tab == Tab::Review && app.answer.is_some() {
                match key.code {
                    KeyCode::Char('1') => app.answer(1),
                    KeyCode::Char('2') => app.answer(2),
                    KeyCode::Char('3') => app.answer(3),
                    KeyCode::Char('4') => app.answer(4),
                    _ => {}
                }
            } else if app.tab != Tab::Review {
                // 1/2/3 already handled as tab switches above; nothing more
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn move_selection(app: &mut App, delta: i32) {
    if app.tab != Tab::Decks || app.decks.is_empty() {
        return;
    }
    let len = app.decks.len() as i32;
    let next = app
        .deck_state
        .selected()
        .map_or(0, |i| (i as i32 + delta).rem_euclid(len) as usize);
    app.deck_state.select(Some(next));
}

fn render(app: &mut App, frame: &mut ratatui::Frame) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    let tabs = ["1 Decks", "2 Review", "3 Stats"];
    let tab_line = Line::from(
        tabs.iter()
            .enumerate()
            .map(|(i, title)| {
                let active = match app.tab {
                    Tab::Decks => i == 0,
                    Tab::Review => i == 1,
                    Tab::Stats => i == 2,
                };
                Span::styled(
                    format!(" {title} "),
                    if active {
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::LightBlue)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    },
                )
            })
            .collect::<Vec<_>>(),
    );
    frame.render_widget(
        Paragraph::new(tab_line).block(Block::default().borders(Borders::BOTTOM)),
        chunks[0],
    );

    if let Some(err) = app.error.clone() {
        frame.render_widget(
            Paragraph::new(err).style(Style::default().fg(Color::Red)),
            chunks[1],
        );
        return;
    }

    match app.tab {
        Tab::Decks => {
            let items: Vec<ListItem> = app
                .decks
                .iter()
                .map(|deck| ListItem::new(deck.name.as_str()))
                .collect();
            frame.render_stateful_widget(
                List::new(items)
                    .block(Block::default().borders(Borders::ALL).title("Decks"))
                    .highlight_style(Style::default().bg(Color::DarkGray)),
                chunks[1],
                &mut app.deck_state,
            );
        }
        Tab::Review => {
            if app.done {
                frame.render_widget(Paragraph::new("done for now 🎉").centered(), chunks[1]);
                return;
            }
            let Some(card) = app.card.clone() else {
                frame.render_widget(
                    Paragraph::new("press 2 to start reviewing").centered(),
                    chunks[1],
                );
                return;
            };
            let mut text = html_to_text(&card.question);
            if let Some(answer) = &app.answer {
                text.push_str("\n\n───\n\n");
                text.push_str(&html_to_text(&answer.answer));
            } else {
                text.push_str("\n\n[space] show answer");
            }
            if app.answer.is_some() {
                text.push_str("\n\n[1] again  [2] hard  [3] good  [4] easy");
            }
            frame.render_widget(
                Paragraph::new(text)
                    .block(Block::default().borders(Borders::ALL).title("Review"))
                    .wrap(Wrap { trim: false }),
                chunks[1],
            );
        }
        Tab::Stats => {
            let Some(stats) = app.stats.clone() else {
                frame.render_widget(
                    Paragraph::new("no stats yet — review some cards first").centered(),
                    chunks[1],
                );
                return;
            };
            let reviews = stats["reviews"].as_u64().unwrap_or(0);
            let mean = stats["mean_secs"].as_f64().unwrap_or(0.0);
            let total = stats["total_secs"].as_f64().unwrap_or(0.0);
            let header = format!("{reviews} reviews · mean {mean:.1}s · total {total:.0}s");
            let mut lines = vec![Line::from(header), Line::from("")];
            // heat strip: one row of 26 blocks ≈ 26 weeks
            let days = stats["days"].as_array().cloned().unwrap_or_default();
            let recent: Vec<_> = days.iter().rev().take(182).rev().collect();
            let max = recent
                .iter()
                .map(|day| day["reviews"].as_u64().unwrap_or(0))
                .max()
                .unwrap_or(1)
                .max(1);
            let mut heat = vec![];
            for chunk in recent.chunks(7) {
                let week: u64 = chunk
                    .iter()
                    .map(|day| day["reviews"].as_u64().unwrap_or(0))
                    .sum();
                let t = (week as f64 / (7 * max) as f64).clamp(0.0, 1.0);
                let color = if week == 0 {
                    Color::DarkGray
                } else {
                    Color::Rgb(
                        (30.0 + t * 92.0) as u8,
                        (33.0 + t * 129.0) as u8,
                        (40.0 + t * 207.0) as u8,
                    )
                };
                heat.push(Span::styled("██", Style::default().fg(color)));
            }
            lines.push(Line::from(heat));
            lines.push(Line::from(""));
            lines.push(Line::from("slowest cards:"));
            if let Some(slowest) = stats["slowest"].as_array() {
                for card in slowest.iter().take(10) {
                    lines.push(Line::from(format!(
                        "  #{} · {} reps · mean {:.1}s · worst {:.1}s",
                        card["card_id"],
                        card["reviews"],
                        card["mean_secs"].as_f64().unwrap_or(0.0),
                        card["max_secs"].as_f64().unwrap_or(0.0),
                    )));
                }
            }
            frame.render_widget(
                Paragraph::new(lines)
                    .block(Block::default().borders(Borders::ALL).title("Stats"))
                    .wrap(Wrap { trim: false }),
                chunks[1],
            );
        }
    }
}
