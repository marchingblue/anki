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
use klep_core::html_to_text;
use klep_core::AnswerView;
use klep_core::BrowserRow;
use klep_core::CardView;
use klep_core::Core;
use klep_core::DeckCounts;
use klep_core::DeckInfo;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
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

#[derive(PartialEq, Eq, Clone, Copy)]
enum Tab {
    Decks,
    Review,
    Browse,
    Stats,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum EditTarget {
    Search,
    Tag,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum DeckEdit {
    New,
    Rename(i64),
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum PathAction {
    Import,
    Export,
}

struct App {
    core: Option<Core>,
    error: Option<String>,
    tab: Tab,
    decks: Vec<DeckInfo>,
    deck_overview: Vec<DeckCounts>,
    deck_state: ListState,
    deck_input: String,
    deck_edit: Option<DeckEdit>,
    current_deck: Option<String>,
    confirm_deck_delete: bool,
    card: Option<CardView>,
    answer: Option<AnswerView>,
    shown_at: Option<Instant>,
    done: bool,
    stats: Option<serde_json::Value>,
    query: String,
    editing: Option<EditTarget>,
    tag_input: String,
    rows: Vec<BrowserRow>,
    row_state: ListState,
    selected: std::collections::HashSet<i64>,
    confirm_delete: bool,
    move_deck_idx: usize,
    marked: bool,
    flag: u32,
    path_input: String,
    path_action: Option<PathAction>,
    notice: Option<String>,
}

impl App {
    fn new() -> Self {
        let mut app = App {
            core: None,
            error: None,
            tab: Tab::Decks,
            decks: vec![],
            deck_overview: vec![],
            deck_state: ListState::default(),
            deck_input: String::new(),
            deck_edit: None,
            current_deck: None,
            confirm_deck_delete: false,
            card: None,
            answer: None,
            shown_at: None,
            done: false,
            stats: None,
            query: String::new(),
            editing: None,
            tag_input: String::new(),
            rows: vec![],
            row_state: ListState::default(),
            selected: Default::default(),
            confirm_delete: false,
            move_deck_idx: 0,
            marked: false,
            flag: 0,
            path_input: String::new(),
            path_action: None,
            notice: None,
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
        let overview = self.core.as_mut().map(|core| core.deck_overview());
        if let Some(result) = overview {
            if let Some(decks) = self.fail(result) {
                self.deck_overview = decks;
                if self.deck_overview.is_empty() {
                    self.deck_state.select(None);
                } else {
                    let last = self.deck_overview.len() - 1;
                    let selected = self.deck_state.selected().unwrap_or(0).min(last);
                    self.deck_state.select(Some(selected));
                }
            }
        }
    }

    fn start_path(&mut self, action: PathAction) {
        self.path_input.clear();
        self.path_action = Some(action);
    }

    fn commit_path(&mut self) {
        let Some(action) = self.path_action.take() else {
            return;
        };
        let path = self.path_input.trim().to_string();
        if path.is_empty() {
            self.error = Some("path required".to_string());
            return;
        }
        let result = self.core.as_mut().map(|core| match action {
            PathAction::Import => core
                .import_apkg(&path)
                .map(|count| format!("imported {count} new notes")),
            PathAction::Export => core
                .export_apkg(&path, "")
                .map(|count| format!("exported {count} notes")),
        });
        if let Some(result) = result {
            match result {
                Ok(message) => {
                    self.error = None;
                    self.notice = Some(message);
                    self.refresh_decks();
                }
                Err(err) => self.error = Some(err.to_string()),
            }
        }
    }

    fn start_new_deck(&mut self) {
        self.deck_input.clear();
        self.deck_edit = Some(DeckEdit::New);
    }

    fn start_rename_deck(&mut self) {
        let Some(idx) = self.deck_state.selected() else {
            return;
        };
        let Some(deck) = self.deck_overview.get(idx) else {
            return;
        };
        self.deck_input = deck.name.clone();
        self.deck_edit = Some(DeckEdit::Rename(deck.id));
    }

    fn commit_deck_edit(&mut self) {
        let Some(edit) = self.deck_edit.take() else {
            return;
        };
        let name = self.deck_input.trim().to_string();
        if name.is_empty() {
            self.error = Some("deck name required".to_string());
            return;
        }
        let result = match edit {
            DeckEdit::New => self
                .core
                .as_mut()
                .map(|core| core.add_deck(&name).map(|_| ())),
            DeckEdit::Rename(id) => self.core.as_mut().map(|core| core.rename_deck(id, &name)),
        };
        if let Some(result) = result {
            if self.fail(result).is_some() {
                self.refresh_decks();
            }
        }
    }

    fn delete_selected_deck(&mut self) {
        let Some(idx) = self.deck_state.selected() else {
            return;
        };
        let Some(deck) = self.deck_overview.get(idx).cloned() else {
            return;
        };
        if !self.confirm_deck_delete {
            self.confirm_deck_delete = true;
            self.error = Some(format!(
                "press d again to delete {} and its cards",
                deck.name
            ));
            return;
        }
        self.confirm_deck_delete = false;
        let result = self.core.as_mut().map(|core| core.delete_deck(deck.id));
        if let Some(result) = result {
            if self.fail(result).is_some() {
                self.refresh_decks();
            }
        }
    }

    fn next(&mut self) {
        self.card = None;
        self.answer = None;
        self.shown_at = None;
        self.done = false;
        self.marked = false;
        self.flag = 0;
        let next = self.core.as_mut().map(|core| core.next_card());
        if let Some(result) = next {
            match self.fail(result) {
                Some(Some(card)) => {
                    self.marked = self
                        .core
                        .as_mut()
                        .and_then(|core| core.is_marked(card.id).ok())
                        .unwrap_or(false);
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

    fn run_search(&mut self) {
        let query = self.query.clone();
        let rows = self.core.as_mut().map(|core| core.search(&query, 200));
        match rows {
            Some(result) => {
                if let Some(rows) = self.fail(result) {
                    self.rows = rows;
                    self.selected.clear();
                    self.confirm_delete = false;
                    self.row_state
                        .select(if self.rows.is_empty() { None } else { Some(0) });
                }
            }
            None => self.error = Some("no collection open".to_string()),
        }
    }

    fn selected_ids(&self) -> Vec<i64> {
        let mut ids: Vec<i64> = self.selected.iter().copied().collect();
        ids.sort();
        ids
    }

    fn bulk_suspend(&mut self, suspend: bool) {
        let ids = self.selected_ids();
        if ids.is_empty() {
            return;
        }
        let result = self
            .core
            .as_mut()
            .map(|core| core.suspend(&ids, suspend).map(|_| ()));
        if let Some(result) = result {
            if self.fail(result).is_some() {
                self.run_search();
            }
        }
    }

    fn bulk_bury(&mut self) {
        let ids = self.selected_ids();
        if ids.is_empty() {
            return;
        }
        let result = self.core.as_mut().map(|core| core.bury(&ids).map(|_| ()));
        if let Some(result) = result {
            if self.fail(result).is_some() {
                self.run_search();
            }
        }
    }

    fn bulk_delete(&mut self) {
        let ids = self.selected_ids();
        if ids.is_empty() {
            return;
        }
        let result = self
            .core
            .as_mut()
            .map(|core| core.delete_cards(&ids).map(|_| ()));
        if let Some(result) = result {
            if self.fail(result).is_some() {
                self.confirm_delete = false;
                self.run_search();
            }
        }
    }

    fn bulk_tag(&mut self, add: bool) {
        let ids = self.selected_ids();
        if ids.is_empty() || self.tag_input.is_empty() {
            return;
        }
        let tag = self.tag_input.clone();
        let result = self
            .core
            .as_mut()
            .map(|core| core.tag(&ids, &tag, add).map(|_| ()));
        if let Some(result) = result {
            if self.fail(result).is_some() {
                self.run_search();
            }
        }
    }

    fn bulk_move(&mut self) {
        let ids = self.selected_ids();
        if ids.is_empty() || self.decks.is_empty() {
            return;
        }
        self.move_deck_idx %= self.decks.len();
        let deck_id = self.decks[self.move_deck_idx].id;
        let result = self
            .core
            .as_mut()
            .map(|core| core.set_deck(&ids, deck_id).map(|_| ()));
        if let Some(result) = result {
            if self.fail(result).is_some() {
                self.run_search();
            }
        }
    }

    fn review_undo(&mut self) {
        let result = self.core.as_mut().map(|core| core.undo());
        if let Some(result) = result {
            if self.fail(result).is_some() {
                self.done = false;
                self.next();
                self.refresh_decks();
            }
        }
    }

    fn review_suspend(&mut self) {
        let Some(card) = self.card.clone() else {
            return;
        };
        let result = self
            .core
            .as_mut()
            .map(|core| core.suspend(&[card.id], true));
        if let Some(result) = result {
            if self.fail(result).is_some() {
                self.done = false;
                self.next();
                self.refresh_decks();
            }
        }
    }

    fn review_bury(&mut self) {
        let Some(card) = self.card.clone() else {
            return;
        };
        let result = self.core.as_mut().map(|core| core.bury(&[card.id]));
        if let Some(result) = result {
            if self.fail(result).is_some() {
                self.done = false;
                self.next();
                self.refresh_decks();
            }
        }
    }

    fn review_mark(&mut self) {
        let Some(card) = self.card.clone() else {
            return;
        };
        let result = self.core.as_mut().map(|core| core.toggle_mark(card.id));
        if let Some(result) = result {
            if let Some(marked) = self.fail(result) {
                self.marked = marked;
            }
        }
    }

    fn review_flag(&mut self) {
        let Some(card) = self.card.clone() else {
            return;
        };
        self.flag = (self.flag + 1) % 8;
        let flag = self.flag;
        let result = self.core.as_mut().map(|core| core.flag_card(card.id, flag));
        if let Some(result) = result {
            self.fail(result);
        }
    }

    /// Edit the current review card in `$EDITOR`. Returns an error string
    /// for display, if anything failed.
    fn edit_in_editor(&mut self) -> Option<String> {
        let card_id = self.card.as_ref()?.id;
        let core = self.core.as_mut()?;
        let data = core
            .edit_data(card_id)
            .map_err(|err| err.to_string())
            .ok()?;
        let path = std::env::temp_dir().join(format!("klepci-edit-{}.txt", data.note_id));
        let template = render_edit_template(
            &data.notetype_name,
            "",
            &data.tags,
            &data.field_names,
            &data.fields,
        );
        if let Err(err) = std::fs::write(&path, template) {
            return Some(err.to_string());
        }
        if let Err(err) = run_editor(&path) {
            return Some(err);
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|err| err.to_string())
            .ok()?;
        let _ = std::fs::remove_file(&path);
        let (fields, tags, _, _) = parse_edit_template(&text, &data.field_names).ok()?;
        core.save_note(data.note_id, fields, &tags)
            .map_err(|err| err.to_string())
            .err()
    }

    /// Add a note via `$EDITOR` (same template, empty fields).
    fn add_in_editor(&mut self) -> Option<String> {
        let core = self.core.as_mut()?;
        let notetypes = core.notetype_names().map_err(|err| err.to_string()).ok()?;
        let fallback = "Basic".to_string();
        let notetype = notetypes.first().unwrap_or(&fallback);
        let field_names = core
            .field_names(notetype)
            .map_err(|err| err.to_string())
            .ok()?;
        let empty = vec![String::new(); field_names.len()];
        let path = std::env::temp_dir().join(format!(
            "klepci-add-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |time| time.as_nanos())
        ));
        let template = render_edit_template(notetype, "Default", "", &field_names, &empty);
        if let Err(err) = std::fs::write(&path, template) {
            return Some(err.to_string());
        }
        if let Err(err) = run_editor(&path) {
            return Some(err);
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|err| err.to_string())
            .ok()?;
        let _ = std::fs::remove_file(&path);
        let (fields, tags, _notetype, deck_name) = parse_edit_template(&text, &field_names).ok()?;
        if fields.iter().all(|field| field.trim().is_empty()) {
            return Some("empty note, not added".to_string());
        }
        let deck_id = self
            .decks
            .iter()
            .find(|deck| deck.name == deck_name)
            .map(|deck| deck.id)
            .unwrap_or(1);
        core.add_note(notetype, deck_id, fields, &tags)
            .map_err(|err| err.to_string())
            .err()
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
            app.notice = None;
            // file path input eats all keys first
            if app.path_action.is_some() {
                match key.code {
                    KeyCode::Esc => app.path_action = None,
                    KeyCode::Enter => app.commit_path(),
                    KeyCode::Backspace => {
                        app.path_input.pop();
                    }
                    KeyCode::Char(ch) => app.path_input.push(ch),
                    _ => {}
                }
                continue;
            }
            // deck name editing eats all keys first
            if app.deck_edit.is_some() {
                match key.code {
                    KeyCode::Esc => app.deck_edit = None,
                    KeyCode::Enter => app.commit_deck_edit(),
                    KeyCode::Backspace => {
                        app.deck_input.pop();
                    }
                    KeyCode::Char(ch) => app.deck_input.push(ch),
                    _ => {}
                }
                continue;
            }
            // text editing (search query / tag input) eats all keys first
            if let Some(target) = app.editing {
                match key.code {
                    KeyCode::Esc => app.editing = None,
                    KeyCode::Enter => {
                        app.editing = None;
                        match target {
                            EditTarget::Search => app.run_search(),
                            EditTarget::Tag => app.bulk_tag(true),
                        }
                    }
                    KeyCode::Backspace => {
                        match target {
                            EditTarget::Search => app.query.pop(),
                            EditTarget::Tag => app.tag_input.pop(),
                        };
                    }
                    KeyCode::Char(ch) => match target {
                        EditTarget::Search => app.query.push(ch),
                        EditTarget::Tag => app.tag_input.push(ch),
                    },
                    _ => {}
                }
                continue;
            }
            // (answer visible: digits rate instead of switching tabs, see below)
            let rating_mode = app.tab == Tab::Review && app.answer.is_some();
            match key.code {
                KeyCode::Char('q') if !rating_mode => break,
                KeyCode::Char('1') if !rating_mode => {
                    app.tab = Tab::Decks;
                }
                KeyCode::Char('2') if !rating_mode => {
                    app.tab = Tab::Review;
                    if app.card.is_none() {
                        app.next();
                    }
                }
                KeyCode::Char('3') if !rating_mode => {
                    app.tab = Tab::Browse;
                }
                KeyCode::Char('4') if !rating_mode => {
                    app.tab = Tab::Stats;
                    app.load_stats();
                }
                KeyCode::Tab => {
                    app.tab = match app.tab {
                        Tab::Decks => Tab::Review,
                        Tab::Review => Tab::Browse,
                        Tab::Browse => Tab::Stats,
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
                        open_selected_deck(&mut app);
                    } else if app.tab == Tab::Review && app.answer.is_none() {
                        app.show();
                    }
                }
                KeyCode::Char(' ') => match app.tab {
                    Tab::Review => app.show(),
                    Tab::Browse => toggle_browse_selection(&mut app),
                    _ => {}
                },
                KeyCode::Char('n') if app.tab == Tab::Decks => app.start_new_deck(),
                KeyCode::Char('i') if app.tab == Tab::Decks => app.start_path(PathAction::Import),
                KeyCode::Char('x') if app.tab == Tab::Decks => app.start_path(PathAction::Export),
                KeyCode::Char('r') if app.tab == Tab::Decks => app.start_rename_deck(),
                KeyCode::Char('d') if app.tab == Tab::Decks => app.delete_selected_deck(),
                KeyCode::Char('u') if app.tab == Tab::Review => app.review_undo(),
                KeyCode::Char('s') if app.tab == Tab::Review => app.review_suspend(),
                KeyCode::Char('b') if app.tab == Tab::Review => app.review_bury(),
                KeyCode::Char('m') if app.tab == Tab::Review => app.review_mark(),
                KeyCode::Char('f') if app.tab == Tab::Review => app.review_flag(),
                KeyCode::Char('E') if app.tab == Tab::Review => match app.edit_in_editor() {
                    Some(err) => app.error = Some(err),
                    None => app.show(),
                },
                KeyCode::Char('A') if app.tab == Tab::Review => {
                    if let Some(err) = app.add_in_editor() {
                        app.error = Some(err);
                    }
                }
                KeyCode::Char('/') if app.tab == Tab::Browse => {
                    app.editing = Some(EditTarget::Search);
                }
                KeyCode::Char('a') if app.tab == Tab::Browse => {
                    app.selected = app.rows.iter().map(|row| row.id).collect();
                }
                KeyCode::Char('s') if app.tab == Tab::Browse => app.bulk_suspend(true),
                KeyCode::Char('u') if app.tab == Tab::Browse => app.bulk_suspend(false),
                KeyCode::Char('b') if app.tab == Tab::Browse => app.bulk_bury(),
                KeyCode::Char('d') if app.tab == Tab::Browse => {
                    if app.confirm_delete {
                        app.bulk_delete();
                    } else {
                        app.confirm_delete = true;
                    }
                }
                KeyCode::Char('t') if app.tab == Tab::Browse => {
                    app.editing = Some(EditTarget::Tag);
                }
                KeyCode::Char('T') if app.tab == Tab::Browse => app.bulk_tag(false),
                KeyCode::Char('m') if app.tab == Tab::Browse => app.bulk_move(),
                KeyCode::Char('M') if app.tab == Tab::Browse && !app.decks.is_empty() => {
                    app.move_deck_idx = (app.move_deck_idx + 1) % app.decks.len();
                }
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
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

/// Leave the TUI, run `$EDITOR` (or vi) on `path`, come back.
fn run_editor(path: &std::path::Path) -> Result<(), String> {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    disable_raw_mode().map_err(|err| err.to_string())?;
    execute!(io::stdout(), LeaveAlternateScreen).map_err(|err| err.to_string())?;
    let status = std::process::Command::new(&editor)
        .arg(path)
        .status()
        .map_err(|err| format!("cannot run {editor}: {err}"))?;
    enable_raw_mode().map_err(|err| err.to_string())?;
    execute!(io::stdout(), EnterAlternateScreen).map_err(|err| err.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{editor} exited with {status}"))
    }
}

fn open_selected_deck(app: &mut App) {
    let Some(idx) = app.deck_state.selected() else {
        return;
    };
    let Some(deck) = app.deck_overview.get(idx).cloned() else {
        return;
    };
    let result = app.core.as_mut().map(|core| core.select_deck(deck.id));
    if let Some(result) = result {
        if app.fail(result).is_some() {
            app.current_deck = Some(deck.name);
            app.tab = Tab::Review;
            app.next();
        }
    }
}

fn move_selection(app: &mut App, delta: i32) {
    if app.tab == Tab::Decks && !app.decks.is_empty() {
        let len = app.decks.len() as i32;
        let next = app
            .deck_state
            .selected()
            .map_or(0, |i| (i as i32 + delta).rem_euclid(len) as usize);
        app.deck_state.select(Some(next));
    } else if app.tab == Tab::Browse && !app.rows.is_empty() {
        let len = app.rows.len() as i32;
        let next = app
            .row_state
            .selected()
            .map_or(0, |i| (i as i32 + delta).rem_euclid(len) as usize);
        app.row_state.select(Some(next));
    }
}

fn toggle_browse_selection(app: &mut App) {
    if let Some(idx) = app.row_state.selected() {
        if let Some(row) = app.rows.get(idx) {
            if !app.selected.remove(&row.id) {
                app.selected.insert(row.id);
            }
        }
    }
}

/// Edit-template format: `# key: value` headers, then `=== Field ===`
/// sections in field order. Round-tested below.
fn render_edit_template(
    notetype: &str,
    deck: &str,
    tags: &str,
    field_names: &[String],
    fields: &[String],
) -> String {
    let mut out = String::from("# klepci note — lines starting with # are ignored\n");
    out.push_str(&format!(
        "# notetype: {notetype}\n# deck: {deck}\n# tags: {tags}\n"
    ));
    for (name, field) in field_names.iter().zip(fields.iter()) {
        out.push_str(&format!("=== {name} ===\n{field}\n"));
    }
    out
}

fn parse_edit_template(
    text: &str,
    field_names: &[String],
) -> Result<(Vec<String>, String, String, String), String> {
    let mut notetype = String::new();
    let mut deck = String::new();
    let mut tags = String::new();
    let mut fields: Vec<String> = Vec::new();
    let mut current: Option<usize> = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("# notetype:") {
            notetype = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("# deck:") {
            deck = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("# tags:") {
            tags = rest.trim().to_string();
        } else if line.starts_with("#") {
            continue;
        } else if line.starts_with("===") && line.ends_with("===") {
            let name = line.trim_matches('=').trim();
            match field_names.iter().position(|known| known == name) {
                Some(idx) => {
                    if fields.len() != idx {
                        return Err(format!("fields out of order at {name}"));
                    }
                    fields.push(String::new());
                    current = Some(idx);
                }
                None => return Err(format!("unknown field {name}")),
            }
        } else if let Some(idx) = current {
            if !fields[idx].is_empty() {
                fields[idx].push('\n');
            }
            fields[idx].push_str(line);
        }
    }
    if fields.len() != field_names.len() {
        return Err(format!(
            "expected {} fields, found {}",
            field_names.len(),
            fields.len()
        ));
    }
    Ok((fields, tags, notetype, deck))
}

fn render(app: &mut App, frame: &mut ratatui::Frame) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    let tabs = ["1 Decks", "2 Review", "3 Browse", "4 Stats"];
    let tab_line = Line::from(
        tabs.iter()
            .enumerate()
            .map(|(i, title)| {
                let active = match app.tab {
                    Tab::Decks => i == 0,
                    Tab::Review => i == 1,
                    Tab::Browse => i == 2,
                    Tab::Stats => i == 3,
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
                .deck_overview
                .iter()
                .map(|deck| {
                    let indent = "  ".repeat(deck.level.saturating_sub(1) as usize);
                    let marker = if deck.filtered { "▣ " } else { "" };
                    let counts = format!(
                        "{} new · {} learn · {} review · {} cards",
                        deck.new, deck.learn, deck.review, deck.total
                    );
                    let mut spans = vec![
                        Span::raw(indent),
                        Span::styled(
                            format!("{marker}{}", deck.name),
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                        Span::raw("  "),
                    ];
                    if deck.new + deck.learn + deck.review == 0 {
                        spans.push(Span::styled(counts, Style::default().fg(Color::DarkGray)));
                    } else {
                        spans.push(Span::styled(counts, Style::default().fg(Color::Cyan)));
                    }
                    ListItem::new(Line::from(spans))
                })
                .collect();
            frame.render_stateful_widget(
                List::new(items)
                    .block(Block::default().borders(Borders::ALL).title(
                        "Decks — n new · r rename · d delete · i import · x export · Enter review",
                    ))
                    .highlight_style(Style::default().bg(Color::DarkGray)),
                chunks[1],
                &mut app.deck_state,
            );
            if let Some(edit) = app.deck_edit {
                let label = match edit {
                    DeckEdit::New => "New deck: ",
                    DeckEdit::Rename(_) => "Rename deck: ",
                };
                let prompt = format!("{label}{}_", app.deck_input);
                let area = Rect {
                    x: chunks[1].x + 1,
                    y: chunks[1].bottom().saturating_sub(2),
                    width: chunks[1].width.saturating_sub(2),
                    height: 1,
                };
                frame.render_widget(
                    Paragraph::new(prompt).style(Style::default().fg(Color::Yellow)),
                    area,
                );
            }
            if let Some(notice) = app.notice.clone() {
                let area = Rect {
                    x: chunks[1].x + 1,
                    y: chunks[1].bottom().saturating_sub(2),
                    width: chunks[1].width.saturating_sub(2),
                    height: 1,
                };
                frame.render_widget(
                    Paragraph::new(notice).style(Style::default().fg(Color::Green)),
                    area,
                );
            }
            if let Some(action) = app.path_action {
                let label = match action {
                    PathAction::Import => "Import .apkg: ",
                    PathAction::Export => "Export to .apkg: ",
                };
                let prompt = format!("{label}{}_", app.path_input);
                let area = Rect {
                    x: chunks[1].x + 1,
                    y: chunks[1].bottom().saturating_sub(2),
                    width: chunks[1].width.saturating_sub(2),
                    height: 1,
                };
                frame.render_widget(
                    Paragraph::new(prompt).style(Style::default().fg(Color::Yellow)),
                    area,
                );
            }
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
            let mut text = String::new();
            if let Some(deck) = &app.current_deck {
                text.push_str(&format!("[{deck}]\n\n"));
            }
            text.push_str(&html_to_text(&card.question));
            if let Some(answer) = &app.answer {
                text.push_str("\n\n───\n\n");
                text.push_str(&html_to_text(&answer.answer));
            } else {
                text.push_str("\n\n[space] show answer");
            }
            if app.answer.is_some() {
                text.push_str("\n\n[1] again  [2] hard  [3] good  [4] easy");
            }
            text.push_str("\n[u] undo  [s] suspend  [b] bury  [m] mark  [f] flag");
            text.push_str(&format!(
                "  (flag {}, {})",
                app.flag,
                if app.marked { "marked" } else { "unmarked" }
            ));
            text.push_str("\n[E] edit in $EDITOR  [A] add note");
            frame.render_widget(
                Paragraph::new(text)
                    .block(Block::default().borders(Borders::ALL).title("Review"))
                    .wrap(Wrap { trim: false }),
                chunks[1],
            );
        }
        Tab::Browse => {
            let area = chunks[1];
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(0),
                    Constraint::Length(3),
                ])
                .split(area);
            let query_line = match app.editing {
                Some(EditTarget::Search) => format!("query: {}▌", app.query),
                Some(EditTarget::Tag) => format!("tag: {}▌ (enter=add)", app.tag_input),
                None => format!("query: {}  [/] edit", app.query),
            };
            frame.render_widget(
                Paragraph::new(query_line)
                    .block(Block::default().borders(Borders::ALL).title("Browse")),
                layout[0],
            );
            let items: Vec<ListItem> = app
                .rows
                .iter()
                .map(|row| {
                    let mark = if app.selected.contains(&row.id) {
                        "[x]"
                    } else {
                        "[ ]"
                    };
                    ListItem::new(format!("{mark} [{}] {}", row.deck, row.preview))
                })
                .collect();
            frame.render_stateful_widget(
                List::new(items)
                    .block(Block::default().borders(Borders::ALL))
                    .highlight_style(Style::default().bg(Color::DarkGray)),
                layout[1],
                &mut app.row_state,
            );
            let move_target = app
                .decks
                .get(app.move_deck_idx % app.decks.len().max(1))
                .map_or("-".to_string(), |deck| deck.name.clone());
            let delete_hint = if app.confirm_delete {
                "[d] CONFIRM delete"
            } else {
                "[d] delete"
            };
            frame.render_widget(
                Paragraph::new(format!(
                    "{} sel · [space] sel [a] all [s] susp [u] unsusp [b] bury {} [t] tag [T] untag [m] move→{} [M] target",
                    app.selected.len(),
                    delete_hint,
                    move_target,
                )),
                layout[2],
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

#[cfg(test)]
mod test {
    use super::{parse_edit_template, render_edit_template};

    #[test]
    fn template_roundtrip() {
        let names = vec!["Front".to_string(), "Back".to_string()];
        let fields = vec!["q\nmulti".to_string(), "a".to_string()];
        let text = render_edit_template("Basic", "Default", "t1 t2", &names, &fields);
        let (back_fields, tags, notetype, deck) = parse_edit_template(&text, &names).unwrap();
        assert_eq!(back_fields, fields);
        assert_eq!(tags, "t1 t2");
        assert_eq!(notetype, "Basic");
        assert_eq!(deck, "Default");
    }

    #[test]
    fn template_rejects_drift() {
        let names = vec!["Front".to_string(), "Back".to_string()];
        assert!(parse_edit_template("=== Front ===\nx\n", &names).is_err());
        assert!(parse_edit_template("=== Nope ===\nx\n=== Back ===\ny\n", &names).is_err());
    }
}
