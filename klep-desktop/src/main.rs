//! klepci desktop: minimalist egui frontend over [`klep_core::Core`].
//!
//! Three tabs, nothing else: Decks, Review, Stats. Review is text-only in
//! v1 (images show as placeholders); media rendering lands in v1.1.

use std::path::PathBuf;
use std::time::Instant;

use eframe::egui;
use klep_core::html_to_text;
use klep_core::AnswerView;
use klep_core::CardView;
use klep_core::Core;
use klep_core::DeckInfo;

fn default_collection_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    #[cfg(not(target_os = "windows"))]
    let base = std::env::var("HOME")
        .map_or_else(|_| ".".to_string(), |home| format!("{home}/.local/share"));
    let dir = PathBuf::from(base).join("klepci");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("collection.anki2")
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Tab {
    Decks,
    Review,
    Browse,
    Add,
    Stats,
}

enum KeyAction {
    Show,
    Edit,
    Answer(u8),
    Undo,
    Suspend,
    Bury,
    Mark,
}

struct KlepciApp {
    core: Option<Core>,
    error: Option<String>,
    tab: Tab,
    decks: Vec<DeckInfo>,
    deck_overview: Vec<klep_core::DeckCounts>,
    current_deck: Option<String>,
    new_deck_name: String,
    apkg_path: String,
    apkg_status: Option<String>,
    options: Option<klep_core::DeckOptions>,
    options_status: Option<String>,
    renaming: Option<(i64, String)>,
    confirm_deck_delete: Option<i64>,
    deck_status: Option<String>,
    card: Option<CardView>,
    answer: Option<AnswerView>,
    marked: bool,
    shown_at: Option<Instant>,
    done_message: Option<String>,
    stats: Option<serde_json::Value>,
    graphs: Option<klep_core::GraphsData>,
    query: String,
    rows: Vec<klep_core::BrowserRow>,
    selected: std::collections::HashSet<i64>,
    tag_input: String,
    find_text: String,
    replace_text: String,
    find_field: String,
    browse_status: Option<String>,
    move_deck: Option<i64>,
    confirm_delete: bool,
    editing: Option<EditBuf>,
    add_notetype: Option<String>,
    add_notetypes: Vec<String>,
    add_field_names: Vec<String>,
    add_deck: Option<i64>,
    add_fields: Vec<String>,
    add_tags: String,
    add_status: Option<String>,
}

#[derive(Clone)]
struct EditBuf {
    note_id: i64,
    field_names: Vec<String>,
    fields: Vec<String>,
    tags: String,
}

impl KlepciApp {
    fn new() -> Self {
        let mut app = KlepciApp {
            core: None,
            error: None,
            tab: Tab::Decks,
            decks: vec![],
            deck_overview: vec![],
            current_deck: None,
            new_deck_name: String::new(),
            apkg_path: String::new(),
            apkg_status: None,
            options: None,
            options_status: None,
            renaming: None,
            confirm_deck_delete: None,
            deck_status: None,
            card: None,
            answer: None,
            marked: false,
            shown_at: None,
            done_message: None,
            stats: None,
            graphs: None,
            query: String::new(),
            rows: vec![],
            selected: Default::default(),
            tag_input: String::new(),
            find_text: String::new(),
            replace_text: String::new(),
            find_field: String::new(),
            browse_status: None,
            move_deck: None,
            confirm_delete: false,
            editing: None,
            add_notetype: None,
            add_notetypes: vec![],
            add_field_names: vec![],
            add_deck: None,
            add_fields: vec![],
            add_tags: String::new(),
            add_status: None,
        };
        match Core::open(&default_collection_path()) {
            Ok(core) => {
                app.core = Some(core);
                app.refresh_decks();
            }
            Err(err) => app.error = Some(err.to_string()),
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
        match decks {
            Some(result) => {
                if let Some(decks) = self.fail(result) {
                    self.decks = decks;
                }
            }
            None => self.error = Some("no collection open".to_string()),
        }
        self.refresh_deck_overview();
    }

    fn refresh_deck_overview(&mut self) {
        let overview = self.core.as_mut().map(|core| core.deck_overview());
        match overview {
            Some(result) => {
                if let Some(decks) = self.fail(result) {
                    self.deck_overview = decks;
                }
            }
            None => self.error = Some("no collection open".to_string()),
        }
    }

    fn open_options(&mut self, deck_id: i64) {
        let result = self.core.as_mut().map(|core| core.deck_options(deck_id));
        match result {
            Some(result) => {
                if let Some(options) = self.fail(result) {
                    self.options = Some(options);
                    self.options_status = None;
                }
            }
            None => self.error = Some("no collection open".to_string()),
        }
    }

    fn save_options(&mut self) {
        let Some(options) = self.options.clone() else {
            return;
        };
        let result = self
            .core
            .as_mut()
            .map(|core| core.save_deck_options(&options));
        if let Some(result) = result {
            if self.fail(result).is_some() {
                self.options_status = Some("options saved".to_string());
                self.refresh_deck_overview();
            }
        }
    }

    fn import_apkg(&mut self) {
        let path = self.apkg_path.trim().to_string();
        if path.is_empty() {
            self.apkg_status = Some("enter a path to an .apkg".to_string());
            return;
        }
        let result = self.core.as_mut().map(|core| core.import_apkg(&path));
        match result {
            Some(result) => {
                if let Some(count) = self.fail(result) {
                    self.apkg_status = Some(format!("imported {count} new notes"));
                    self.refresh_decks();
                }
            }
            None => self.error = Some("no collection open".to_string()),
        }
    }

    fn export_apkg(&mut self) {
        let path = self.apkg_path.trim().to_string();
        if path.is_empty() {
            self.apkg_status = Some("enter an output path ending in .apkg".to_string());
            return;
        }
        let result = self.core.as_mut().map(|core| core.export_apkg(&path, ""));
        match result {
            Some(result) => {
                if let Some(count) = self.fail(result) {
                    self.apkg_status = Some(format!("exported {count} notes"));
                }
            }
            None => self.error = Some("no collection open".to_string()),
        }
    }

    fn create_deck(&mut self) {
        let name = self.new_deck_name.trim().to_string();
        if name.is_empty() {
            self.deck_status = Some("deck name required".to_string());
            return;
        }
        let result = self.core.as_mut().map(|core| core.add_deck(&name));
        match result {
            Some(result) => {
                if let Some(id) = self.fail(result) {
                    self.deck_status = Some(format!("created deck #{id}"));
                    self.new_deck_name.clear();
                    self.refresh_decks();
                }
            }
            None => self.error = Some("no collection open".to_string()),
        }
    }

    fn save_rename(&mut self) {
        let Some((deck_id, name)) = self.renaming.clone() else {
            return;
        };
        let name = name.trim().to_string();
        if name.is_empty() {
            self.deck_status = Some("deck name required".to_string());
            return;
        }
        let result = self
            .core
            .as_mut()
            .map(|core| core.rename_deck(deck_id, &name));
        match result {
            Some(result) => {
                if self.fail(result).is_some() {
                    self.renaming = None;
                    self.refresh_decks();
                }
            }
            None => self.error = Some("no collection open".to_string()),
        }
    }

    fn delete_deck(&mut self, deck_id: i64) {
        let result = self.core.as_mut().map(|core| core.delete_deck(deck_id));
        match result {
            Some(result) => {
                if let Some(count) = self.fail(result) {
                    self.deck_status = Some(format!("deleted deck (+{count} cards)"));
                    self.confirm_deck_delete = None;
                    self.refresh_decks();
                }
            }
            None => self.error = Some("no collection open".to_string()),
        }
    }

    fn next(&mut self) {
        self.card = None;
        self.answer = None;
        self.shown_at = None;
        self.done_message = None;
        self.marked = false;
        let next = self.core.as_mut().map(|core| core.next_card());
        match next {
            Some(result) => match self.fail(result) {
                Some(Some(card)) => {
                    let marked = self
                        .core
                        .as_mut()
                        .and_then(|core| core.is_marked(card.id).ok())
                        .unwrap_or(false);
                    self.card = Some(card);
                    self.shown_at = Some(Instant::now());
                    self.marked = marked;
                }
                Some(None) => self.done_message = Some("done for now 🎉".to_string()),
                None => {}
            },
            None => self.error = Some("no collection open".to_string()),
        }
    }

    fn show(&mut self) {
        let card_id = match &self.card {
            Some(card) => card.id,
            None => return,
        };
        let rendered = self.core.as_mut().map(|core| core.show_answer(card_id));
        match rendered {
            Some(result) => {
                if let Some(answer) = self.fail(result) {
                    self.answer = Some(answer);
                }
            }
            None => self.error = Some("no collection open".to_string()),
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
        match result {
            Some(result) => {
                if self.fail(result).is_some() {
                    self.next();
                    self.refresh_deck_overview();
                }
            }
            None => self.error = Some("no collection open".to_string()),
        }
    }

    fn undo(&mut self) {
        let result = self.core.as_mut().map(|core| core.undo());
        if let Some(result) = result {
            if self.fail(result).is_some() {
                self.done_message = None;
                self.next();
                self.refresh_deck_overview();
            }
        }
    }

    fn suspend_current(&mut self) {
        let Some(card) = self.card.clone() else {
            return;
        };
        let result = self
            .core
            .as_mut()
            .map(|core| core.suspend(&[card.id], true));
        if let Some(result) = result {
            if self.fail(result).is_some() {
                self.done_message = None;
                self.next();
                self.refresh_deck_overview();
            }
        }
    }

    fn bury_current(&mut self) {
        let Some(card) = self.card.clone() else {
            return;
        };
        let result = self.core.as_mut().map(|core| core.bury(&[card.id]));
        if let Some(result) = result {
            if self.fail(result).is_some() {
                self.done_message = None;
                self.next();
                self.refresh_deck_overview();
            }
        }
    }

    fn flag_current(&mut self, flag: u32) {
        let Some(card) = self.card.clone() else {
            return;
        };
        let result = self.core.as_mut().map(|core| core.flag_card(card.id, flag));
        if let Some(result) = result {
            self.fail(result);
        }
    }

    fn mark_current(&mut self) {
        let Some(card) = self.card.clone() else {
            return;
        };
        let result = self.core.as_mut().map(|core| core.toggle_mark(card.id));
        match result {
            Some(result) => {
                if let Some(marked) = self.fail(result) {
                    self.marked = marked;
                }
            }
            None => self.error = Some("no collection open".to_string()),
        }
    }

    fn start_edit(&mut self) {
        let card_id = match &self.card {
            Some(card) => card.id,
            None => return,
        };
        let data = self.core.as_mut().map(|core| core.edit_data(card_id));
        match data {
            Some(result) => {
                if let Some(data) = self.fail(result) {
                    self.editing = Some(EditBuf {
                        note_id: data.note_id,
                        field_names: data.field_names,
                        fields: data.fields,
                        tags: data.tags,
                    });
                }
            }
            None => self.error = Some("no collection open".to_string()),
        }
    }

    fn save_edit(&mut self) {
        let Some(edit) = self.editing.clone() else {
            return;
        };
        let result = self
            .core
            .as_mut()
            .map(|core| core.save_note(edit.note_id, edit.fields.clone(), &edit.tags));
        match result {
            Some(result) => {
                if self.fail(result).is_some() {
                    self.editing = None;
                    self.show();
                }
            }
            None => self.error = Some("no collection open".to_string()),
        }
    }

    fn enter_add(&mut self) {
        let names = self.core.as_mut().map(|core| core.notetype_names());
        match names {
            Some(result) => {
                if let Some(names) = self.fail(result) {
                    self.add_notetypes = names;
                }
            }
            None => self.error = Some("no collection open".to_string()),
        }
    }

    fn refresh_add_fields(&mut self) {
        let notetype = match self.add_notetype.clone() {
            Some(notetype) => notetype,
            None => return,
        };
        let names = self.core.as_mut().map(|core| core.field_names(&notetype));
        match names {
            Some(result) => {
                if let Some(field_names) = self.fail(result) {
                    self.add_fields = vec![String::new(); field_names.len()];
                    self.add_field_names = field_names;
                }
            }
            None => self.error = Some("no collection open".to_string()),
        }
    }

    fn save_add(&mut self) {
        let (Some(notetype), Some(deck_id)) = (self.add_notetype.clone(), self.add_deck) else {
            self.error = Some("pick a notetype and a deck first".to_string());
            return;
        };
        let fields = self.add_fields.clone();
        let tags = self.add_tags.clone();
        let result = self
            .core
            .as_mut()
            .map(|core| core.add_note(&notetype, deck_id, fields, &tags));
        match result {
            Some(result) => {
                if let Some(id) = self.fail(result) {
                    self.add_status = Some(format!("added note #{id}"));
                    for field in &mut self.add_fields {
                        field.clear();
                    }
                    self.add_tags.clear();
                }
            }
            None => self.error = Some("no collection open".to_string()),
        }
    }

    fn load_stats(&mut self) {
        let json = self.core.as_mut().map(|core| core.timing_json("", 10));
        match json {
            Some(result) => {
                self.stats = self
                    .fail(result)
                    .and_then(|text| serde_json::from_str(&text).ok())
            }
            None => {
                self.error = Some("no collection open".to_string());
                self.stats = None;
            }
        }
        let graphs = self.core.as_mut().map(|core| core.graphs("", 365));
        match graphs {
            Some(result) => self.graphs = self.fail(result),
            None => self.graphs = None,
        }
    }

    fn run_search(&mut self) {
        let query = self.query.clone();
        let rows = self.core.as_mut().map(|core| core.search(&query, 200));
        match rows {
            Some(result) => {
                if let Some(rows) = self.fail(result) {
                    self.selected.clear();
                    self.confirm_delete = false;
                    self.rows = rows;
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

    fn run_replace(&mut self) {
        let search = self.query.clone();
        let regex = self.find_text.clone();
        let replacement = self.replace_text.clone();
        let field = {
            let field = self.find_field.trim().to_string();
            if field.is_empty() {
                None
            } else {
                Some(field)
            }
        };
        if regex.is_empty() {
            self.browse_status = Some("find pattern is empty".to_string());
            return;
        }
        let result = self
            .core
            .as_mut()
            .map(|core| core.find_and_replace(&search, &regex, &replacement, field.as_deref()));
        match result {
            Some(result) => {
                if let Some(count) = self.fail(result) {
                    self.browse_status = Some(format!("replaced in {count} notes"));
                    self.run_search();
                }
            }
            None => self.error = Some("no collection open".to_string()),
        }
    }

    fn bulk(&mut self, op: BulkOp) {
        let ids = self.selected_ids();
        if ids.is_empty() {
            return;
        }
        let result = self.core.as_mut().map(|core| match &op {
            BulkOp::Suspend(on) => core.suspend(&ids, *on).map(|_| ()),
            BulkOp::Bury => core.bury(&ids).map(|_| ()),
            BulkOp::Delete => core.delete_cards(&ids).map(|_| ()),
            BulkOp::Move(deck_id) => core.set_deck(&ids, *deck_id).map(|_| ()),
            BulkOp::Tag(tag, add) => core.tag(&ids, tag, *add).map(|_| ()),
        });
        match result {
            Some(result) => {
                if self.fail(result).is_some() {
                    self.confirm_delete = false;
                    self.run_search();
                }
            }
            None => self.error = Some("no collection open".to_string()),
        }
    }
}

enum BulkOp {
    Suspend(bool),
    Bury,
    Delete,
    Move(i64),
    Tag(String, bool),
}

impl eframe::App for KlepciApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // keyboard first (collect, then act to keep borrows clean)
        let mut key_action = None;
        if self.tab == Tab::Review {
            ui.ctx().input(|input| {
                if input.key_pressed(egui::Key::Space) && self.answer.is_none() {
                    key_action = Some(KeyAction::Show);
                }
                if input.key_pressed(egui::Key::E) {
                    key_action = Some(KeyAction::Edit);
                }
                if input.key_pressed(egui::Key::U) {
                    key_action = Some(KeyAction::Undo);
                }
                if input.key_pressed(egui::Key::S) {
                    key_action = Some(KeyAction::Suspend);
                }
                if input.key_pressed(egui::Key::B) {
                    key_action = Some(KeyAction::Bury);
                }
                if input.key_pressed(egui::Key::M) {
                    key_action = Some(KeyAction::Mark);
                }
                for (key, rating) in [
                    (egui::Key::Num1, 1),
                    (egui::Key::Num2, 2),
                    (egui::Key::Num3, 3),
                    (egui::Key::Num4, 4),
                ] {
                    if input.key_pressed(key) && self.answer.is_some() {
                        key_action = Some(KeyAction::Answer(rating));
                    }
                }
            });
        }
        match key_action {
            Some(KeyAction::Show) => self.show(),
            Some(KeyAction::Edit) => self.start_edit(),
            Some(KeyAction::Answer(rating)) => self.answer(rating),
            Some(KeyAction::Undo) => self.undo(),
            Some(KeyAction::Suspend) => self.suspend_current(),
            Some(KeyAction::Bury) => self.bury_current(),
            Some(KeyAction::Mark) => self.mark_current(),
            None => {}
        }

        egui::Panel::top("tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                for (tab, label) in [
                    (Tab::Decks, "Decks"),
                    (Tab::Review, "Review"),
                    (Tab::Browse, "Browse"),
                    (Tab::Add, "Add"),
                    (Tab::Stats, "Stats"),
                ] {
                    if ui.selectable_label(self.tab == tab, label).clicked() {
                        self.tab = tab;
                        if tab == Tab::Review && self.card.is_none() {
                            self.next();
                        }
                        if tab == Tab::Add {
                            self.enter_add();
                        }
                        if tab == Tab::Stats {
                            self.load_stats();
                        }
                    }
                }
            });
        });

        if let Some(err) = self.error.clone() {
            egui::Panel::top("err").show(ui, |ui| {
                ui.colored_label(egui::Color32::from_rgb(247, 118, 142), err);
            });
        }

        egui::CentralPanel::default().show(ui, |ui| {
            ui.add_space(8.0);
            match self.tab {
                Tab::Decks => self.view_decks(ui),
                Tab::Review => self.view_review(ui),
                Tab::Browse => self.view_browse(ui),
                Tab::Add => self.view_add(ui),
                Tab::Stats => self.view_stats(ui),
            }
        });
    }
}

impl KlepciApp {
    fn view_decks(&mut self, ui: &mut egui::Ui) {
        if self.options.is_some() {
            self.view_options(ui);
            return;
        }
        if self.core.is_none() {
            ui.centered_and_justified(|ui| ui.label("no collection open"));
            return;
        }
        ui.horizontal(|ui| {
            ui.add_sized(
                [(ui.available_width() - 110.0).max(120.0), 28.0],
                egui::TextEdit::singleline(&mut self.new_deck_name).hint_text("New deck (A::B)"),
            );
            if ui.button("Create").clicked() {
                self.create_deck();
            }
        });
        if let Some(status) = &self.deck_status.clone() {
            ui.label(egui::RichText::new(status).small().weak());
        }
        ui.horizontal(|ui| {
            ui.add_sized(
                [(ui.available_width() - 190.0).max(120.0), 28.0],
                egui::TextEdit::singleline(&mut self.apkg_path).hint_text("/path/to/deck.apkg"),
            );
            if ui.button("Import .apkg").clicked() {
                self.import_apkg();
            }
            if ui.button("Export .apkg").clicked() {
                self.export_apkg();
            }
        });
        if let Some(status) = &self.apkg_status.clone() {
            ui.label(egui::RichText::new(status).small().weak());
        }
        ui.separator();
        if self.deck_overview.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label("no decks yet — import an .apkg in Anki first");
            });
            return;
        }
        let mut open_deck = None;
        let mut open_options = None;
        let mut start_rename = None;
        let mut request_delete = None;
        let mut save_rename = false;
        let mut cancel_rename = false;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for deck in self.deck_overview.clone() {
                let counts = format!(
                    "{} new · {} learn · {} review",
                    deck.new, deck.learn, deck.review
                );
                if let Some((editing_id, name)) = self.renaming.as_mut() {
                    if *editing_id == deck.id {
                        ui.horizontal(|ui| {
                            ui.add_space(deck.level as f32 * 14.0);
                            ui.add_sized(
                                [(ui.available_width() - 150.0).max(120.0), 28.0],
                                egui::TextEdit::singleline(name),
                            );
                            if ui.button("Save").clicked() {
                                save_rename = true;
                            }
                            if ui.button("Cancel").clicked() {
                                cancel_rename = true;
                            }
                        });
                        continue;
                    }
                }
                ui.horizontal(|ui| {
                    ui.add_space(deck.level.saturating_sub(1) as f32 * 14.0);
                    let title = if deck.filtered {
                        format!("▣ {}", deck.name)
                    } else {
                        deck.name.clone()
                    };
                    let button = egui::Button::new(egui::RichText::new(title).strong())
                        .min_size(egui::vec2((ui.available_width() - 250.0).max(80.0), 34.0));
                    if ui.add(button).clicked() {
                        open_deck = Some(deck.id);
                    }
                    let text = if deck.total == 0 && deck.new + deck.learn + deck.review == 0 {
                        format!("empty · {counts}")
                    } else {
                        format!("{} cards · {counts}", deck.total)
                    };
                    ui.label(egui::RichText::new(text).small().weak());
                    if ui.small_button("Rename").clicked() {
                        start_rename = Some((deck.id, deck.name.clone()));
                    }
                    if ui.small_button("Options").clicked() {
                        open_options = Some(deck.id);
                    }
                    if self.confirm_deck_delete == Some(deck.id) {
                        if ui.small_button("Confirm delete").clicked() {
                            request_delete = Some(deck.id);
                        }
                    } else if ui.small_button("Delete").clicked() {
                        request_delete = Some(deck.id);
                    }
                });
            }
        });
        if let Some(deck_id) = open_deck {
            self.current_deck = self
                .deck_overview
                .iter()
                .find(|deck| deck.id == deck_id)
                .map(|deck| deck.name.clone());
            let result = self.core.as_mut().map(|core| core.select_deck(deck_id));
            if let Some(result) = result {
                if self.fail(result).is_some() {
                    self.tab = Tab::Review;
                    self.next();
                }
            }
        }
        if let Some(deck_id) = open_options {
            self.open_options(deck_id);
        }
        if let Some((id, name)) = start_rename {
            self.renaming = Some((id, name));
            self.confirm_deck_delete = None;
        }
        if save_rename {
            self.save_rename();
        }
        if cancel_rename {
            self.renaming = None;
        }
        if let Some(deck_id) = request_delete {
            if self.confirm_deck_delete == Some(deck_id) {
                self.delete_deck(deck_id);
            } else {
                self.confirm_deck_delete = Some(deck_id);
                self.deck_status = Some("click Delete again to confirm".to_string());
            }
        }
    }

    fn view_options(&mut self, ui: &mut egui::Ui) {
        let mut save = false;
        let mut back = false;
        if let Some(options) = self.options.as_mut() {
            ui.horizontal(|ui| {
                ui.heading("Deck options");
                ui.label(
                    egui::RichText::new(format!("preset: {}", options.preset_name))
                        .small()
                        .weak(),
                );
            });
            ui.label(
                egui::RichText::new(
                    "Changes apply to every deck using this preset, like stock Anki.",
                )
                .small()
                .weak(),
            );
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("options_grid")
                    .num_columns(2)
                    .spacing([18.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("New cards/day");
                        ui.add(egui::DragValue::new(&mut options.new_per_day).range(0..=9999));
                        ui.end_row();

                        ui.label("Maximum reviews/day");
                        ui.add(egui::DragValue::new(&mut options.reviews_per_day).range(0..=99999));
                        ui.end_row();

                        ui.label("Learning steps (minutes)");
                        ui.add(
                            egui::TextEdit::singleline(&mut options.learn_steps)
                                .desired_width(160.0),
                        );
                        ui.end_row();

                        ui.label("Relearning steps");
                        ui.add(
                            egui::TextEdit::singleline(&mut options.relearn_steps)
                                .desired_width(160.0),
                        );
                        ui.end_row();

                        ui.label("Graduating interval (good)");
                        ui.add(
                            egui::DragValue::new(&mut options.graduating_interval_good)
                                .range(1..=9999),
                        );
                        ui.end_row();

                        ui.label("Easy interval");
                        ui.add(
                            egui::DragValue::new(&mut options.graduating_interval_easy)
                                .range(1..=9999),
                        );
                        ui.end_row();

                        ui.label("Easy bonus");
                        ui.add(
                            egui::DragValue::new(&mut options.easy_multiplier)
                                .speed(0.01)
                                .range(1.0..=5.0),
                        );
                        ui.end_row();

                        ui.label("Hard interval");
                        ui.add(
                            egui::DragValue::new(&mut options.hard_multiplier)
                                .speed(0.01)
                                .range(0.5..=1.0),
                        );
                        ui.end_row();

                        ui.label("Interval modifier");
                        ui.add(
                            egui::DragValue::new(&mut options.interval_multiplier)
                                .speed(0.01)
                                .range(0.5..=2.0),
                        );
                        ui.end_row();

                        ui.label("Maximum interval (days)");
                        ui.add(
                            egui::DragValue::new(&mut options.maximum_review_interval)
                                .range(1..=36500),
                        );
                        ui.end_row();

                        ui.label("Minimum lapse interval");
                        ui.add(
                            egui::DragValue::new(&mut options.minimum_lapse_interval)
                                .range(1..=9999),
                        );
                        ui.end_row();

                        ui.label("Leech threshold");
                        ui.add(egui::DragValue::new(&mut options.leech_threshold).range(1..=99));
                        ui.end_row();

                        ui.label("Leech action");
                        egui::ComboBox::from_id_salt("leech_action")
                            .selected_text(match options.leech_action {
                                0 => "Suspend",
                                2 => "Bury",
                                _ => "Tag only",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut options.leech_action, 0, "Suspend");
                                ui.selectable_value(&mut options.leech_action, 1, "Tag only");
                                ui.selectable_value(&mut options.leech_action, 2, "Bury");
                            });
                        ui.end_row();

                        ui.label("FSRS");
                        ui.checkbox(&mut options.fsrs, "use FSRS scheduler");
                        ui.end_row();

                        ui.label("Desired retention");
                        ui.add(
                            egui::DragValue::new(&mut options.desired_retention)
                                .speed(0.005)
                                .range(0.7..=0.99),
                        );
                        ui.end_row();

                        ui.label("Bury");
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut options.bury_new, "new");
                            ui.checkbox(&mut options.bury_reviews, "reviews");
                            ui.checkbox(&mut options.bury_interday_learning, "learning");
                        });
                        ui.end_row();

                        ui.label("Show answer timer");
                        ui.checkbox(&mut options.show_timer, "enabled");
                        ui.end_row();

                        ui.label("Cap answer time (secs)");
                        ui.add(
                            egui::DragValue::new(&mut options.cap_answer_time_to_secs)
                                .range(0..=3600),
                        );
                        ui.end_row();
                    });
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    save = true;
                }
                if ui.button("Back").clicked() {
                    back = true;
                }
                if let Some(status) = &self.options_status {
                    ui.label(egui::RichText::new(status).small().weak());
                }
            });
        }
        if save {
            self.save_options();
        }
        if back {
            self.options = None;
        }
    }

    fn view_review(&mut self, ui: &mut egui::Ui) {
        if self.editing.is_some() {
            self.view_edit(ui);
            return;
        }
        if let Some(done) = &self.done_message.clone() {
            ui.centered_and_justified(|ui| {
                ui.heading(done);
            });
            return;
        }
        let Some(card) = self.card.clone() else {
            ui.centered_and_justified(|ui| {
                ui.label("press Review to start");
            });
            return;
        };
        if let Some(deck) = &self.current_deck {
            ui.label(egui::RichText::new(deck).small().weak());
        }
        let style = ui.ctx().global_style();
        egui::Frame::group(&style).show(ui, |ui| {
            ui.add_space(8.0);
            ui.label(egui::RichText::new(html_to_text(&card.question)).size(20.0));
            if let Some(answer) = &self.answer {
                ui.separator();
                ui.label(egui::RichText::new(html_to_text(&answer.answer)).size(20.0));
            }
            ui.add_space(8.0);
        });
        ui.add_space(12.0);
        if self.answer.is_none() {
            if ui
                .add_sized(
                    [ui.available_width(), 48.0],
                    egui::Button::new("Show answer (space)"),
                )
                .clicked()
            {
                self.show();
            }
            ui.add_space(6.0);
            if ui.small_button("Edit note (E)").clicked() {
                self.start_edit();
            }
        } else {
            ui.horizontal(|ui| {
                for (rating, label) in [(1, "Again"), (2, "Hard"), (3, "Good"), (4, "Easy")] {
                    if ui
                        .add_sized(
                            [(ui.available_width() - 24.0) / 4.0, 52.0],
                            egui::Button::new(format!("{label}\n{rating}")),
                        )
                        .clicked()
                    {
                        self.answer(rating);
                    }
                }
            });
        }
        ui.add_space(10.0);
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            if ui.small_button("Undo (U)").clicked() {
                self.undo();
            }
            if ui.small_button("Suspend (S)").clicked() {
                self.suspend_current();
            }
            if ui.small_button("Bury (B)").clicked() {
                self.bury_current();
            }
            if ui
                .small_button(if self.marked {
                    "Unmark (M)"
                } else {
                    "Mark (M)"
                })
                .clicked()
            {
                self.mark_current();
            }
            ui.label("flag:");
            for flag in 0..=7 {
                if ui.small_button(format!("{flag}")).clicked() {
                    self.flag_current(flag);
                }
            }
        });
    }

    fn view_browse(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let search = ui.add_sized(
                [ui.available_width() - 90.0, 28.0],
                egui::TextEdit::singleline(&mut self.query)
                    .hint_text("search, e.g. deck:Bio tag:exam"),
            );
            if ui
                .add_sized([80.0, 28.0], egui::Button::new("Search"))
                .clicked()
                || search.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter))
            {
                self.run_search();
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_sized(
                [140.0, 24.0],
                egui::TextEdit::singleline(&mut self.find_text).hint_text("find (regex)"),
            );
            ui.add_sized(
                [140.0, 24.0],
                egui::TextEdit::singleline(&mut self.replace_text).hint_text("replace"),
            );
            ui.add_sized(
                [90.0, 24.0],
                egui::TextEdit::singleline(&mut self.find_field).hint_text("field (opt)"),
            );
            if ui.small_button("Replace all").clicked() {
                self.run_replace();
            }
            if let Some(status) = &self.browse_status {
                ui.label(egui::RichText::new(status).small().weak());
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(format!("{} selected", self.selected.len()));
            if ui.small_button("all").clicked() {
                self.selected = self.rows.iter().map(|row| row.id).collect();
            }
            if ui.small_button("none").clicked() {
                self.selected.clear();
            }
            if ui.small_button("suspend").clicked() {
                self.bulk(BulkOp::Suspend(true));
            }
            if ui.small_button("unsuspend").clicked() {
                self.bulk(BulkOp::Suspend(false));
            }
            if ui.small_button("bury").clicked() {
                self.bulk(BulkOp::Bury);
            }
        });
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("move_deck")
                .selected_text(
                    self.move_deck
                        .and_then(|id| self.decks.iter().find(|deck| deck.id == id))
                        .map_or("move to…".to_string(), |deck| deck.name.clone()),
                )
                .show_ui(ui, |ui| {
                    for deck in self.decks.clone() {
                        ui.selectable_value(&mut self.move_deck, Some(deck.id), &deck.name);
                    }
                });
            if ui.small_button("move").clicked() {
                if let Some(deck_id) = self.move_deck {
                    self.bulk(BulkOp::Move(deck_id));
                }
            }
            ui.add_sized(
                [120.0, 22.0],
                egui::TextEdit::singleline(&mut self.tag_input).hint_text("tag"),
            );
            if ui.small_button("+tag").clicked() {
                let tag = self.tag_input.clone();
                if !tag.is_empty() {
                    self.bulk(BulkOp::Tag(tag, true));
                }
            }
            if ui.small_button("−tag").clicked() {
                let tag = self.tag_input.clone();
                if !tag.is_empty() {
                    self.bulk(BulkOp::Tag(tag, false));
                }
            }
            let delete_label = if self.confirm_delete {
                "confirm delete?"
            } else {
                "delete"
            };
            if ui.small_button(delete_label).clicked() {
                if self.confirm_delete {
                    self.bulk(BulkOp::Delete);
                } else {
                    self.confirm_delete = true;
                }
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for row in self.rows.clone() {
                let mut checked = self.selected.contains(&row.id);
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut checked, "").changed() {
                        if checked {
                            self.selected.insert(row.id);
                        } else {
                            self.selected.remove(&row.id);
                        }
                    }
                    ui.label(format!("[{}] {}", row.deck, row.preview));
                });
            }
        });
    }

    fn view_edit(&mut self, ui: &mut egui::Ui) {
        let mut save = false;
        let mut cancel = false;
        if let Some(edit) = self.editing.as_mut() {
            ui.label(format!("editing note #{}", edit.note_id));
            for (name, field) in edit.field_names.clone().iter().zip(edit.fields.iter_mut()) {
                ui.label(name);
                ui.add_sized(
                    [ui.available_width(), 72.0],
                    egui::TextEdit::multiline(field),
                );
            }
            ui.label("tags");
            ui.add_sized(
                [ui.available_width(), 28.0],
                egui::TextEdit::singleline(&mut edit.tags),
            );
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    save = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        }
        if save {
            self.save_edit();
        }
        if cancel {
            self.editing = None;
        }
    }

    fn view_add(&mut self, ui: &mut egui::Ui) {
        if self.decks.is_empty() {
            self.refresh_decks();
        }
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("add_notetype")
                .selected_text(
                    self.add_notetype
                        .clone()
                        .unwrap_or_else(|| "type…".to_string()),
                )
                .show_ui(ui, |ui| {
                    for name in self.add_notetypes.clone() {
                        if ui
                            .selectable_value(&mut self.add_notetype, Some(name.clone()), &name)
                            .clicked()
                        {
                            self.refresh_add_fields();
                        }
                    }
                });
            egui::ComboBox::from_id_salt("add_deck")
                .selected_text(
                    self.add_deck
                        .and_then(|id| self.decks.iter().find(|deck| deck.id == id))
                        .map_or("deck…".to_string(), |deck| deck.name.clone()),
                )
                .show_ui(ui, |ui| {
                    for deck in self.decks.clone() {
                        ui.selectable_value(&mut self.add_deck, Some(deck.id), &deck.name);
                    }
                });
        });
        for (name, field) in self
            .add_field_names
            .clone()
            .iter()
            .zip(self.add_fields.iter_mut())
        {
            ui.label(name);
            ui.add_sized(
                [ui.available_width(), 72.0],
                egui::TextEdit::multiline(field),
            );
        }
        if !self.add_field_names.is_empty() {
            ui.label("tags");
            ui.add_sized(
                [ui.available_width(), 28.0],
                egui::TextEdit::singleline(&mut self.add_tags),
            );
            if ui.button("Add note").clicked() {
                self.save_add();
            }
        }
        if let Some(status) = &self.add_status.clone() {
            ui.label(status);
        }
    }

    fn view_stats(&mut self, ui: &mut egui::Ui) {
        let Some(stats) = self.stats.clone() else {
            ui.centered_and_justified(|ui| {
                ui.label("no stats yet — review some cards first");
            });
            return;
        };
        egui::ScrollArea::vertical().show(ui, |ui| {
            let reviews = stats["reviews"].as_u64().unwrap_or(0);
            let mean = stats["mean_secs"].as_f64().unwrap_or(0.0);
            let total = stats["total_secs"].as_f64().unwrap_or(0.0);
            ui.heading(format!(
                "{reviews} reviews · mean {mean:.1}s · total {total:.0}s"
            ));
            ui.add_space(8.0);

            if let Some(graphs) = self.graphs.clone() {
                draw_graphs(ui, &graphs);
                ui.add_space(10.0);
            }

            ui.label("last 26 weeks");
            let days = stats["days"].as_array().cloned().unwrap_or_default();
            let recent: Vec<_> = days.iter().rev().take(182).rev().cloned().collect();
            let max = recent
                .iter()
                .map(|day| day["reviews"].as_u64().unwrap_or(0))
                .max()
                .unwrap_or(1)
                .max(1);
            let cell = 14.0;
            let weeks = recent.len().div_ceil(7).max(1);
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(weeks as f32 * cell, 7.0 * cell),
                egui::Sense::hover(),
            );
            for (i, day) in recent.iter().enumerate() {
                let count = day["reviews"].as_u64().unwrap_or(0);
                let t = (count as f32 / max as f32).clamp(0.0, 1.0);
                let color = egui::Color32::from_rgb(
                    (30.0 + t * (122.0 - 30.0)) as u8,
                    (33.0 + t * (162.0 - 33.0)) as u8,
                    (40.0 + t * (247.0 - 40.0)) as u8,
                );
                let pos = rect.min + egui::vec2((i / 7) as f32 * cell, (i % 7) as f32 * cell);
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(pos, egui::vec2(cell - 2.0, cell - 2.0)),
                    2.0,
                    color,
                );
            }
            ui.add_space(12.0);
            ui.label("slowest cards");
            egui::Grid::new("slowest").striped(true).show(ui, |ui| {
                ui.label("card");
                ui.label("reps");
                ui.label("mean");
                ui.label("worst");
                ui.end_row();
                if let Some(slowest) = stats["slowest"].as_array() {
                    for card in slowest {
                        ui.label(card["card_id"].to_string());
                        ui.label(card["reviews"].to_string());
                        ui.label(format!("{:.1}s", card["mean_secs"].as_f64().unwrap_or(0.0)));
                        ui.label(format!("{:.1}s", card["max_secs"].as_f64().unwrap_or(0.0)));
                        ui.end_row();
                    }
                }
            });
        });
    }
}

const LEARN_COLOR: egui::Color32 = egui::Color32::from_rgb(224, 175, 104);
const RELEARN_COLOR: egui::Color32 = egui::Color32::from_rgb(247, 118, 142);
const YOUNG_COLOR: egui::Color32 = egui::Color32::from_rgb(122, 162, 247);
const MATURE_COLOR: egui::Color32 = egui::Color32::from_rgb(158, 206, 106);
const FILTERED_COLOR: egui::Color32 = egui::Color32::from_rgb(187, 154, 247);
const ADDED_COLOR: egui::Color32 = egui::Color32::from_rgb(125, 207, 255);
const REVIEW_COLOR: egui::Color32 = egui::Color32::from_rgb(122, 162, 247);

fn legend(ui: &mut egui::Ui, entries: &[(&str, egui::Color32)]) {
    ui.horizontal_wrapped(|ui| {
        for (label, color) in entries {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 2.0, *color);
            ui.label(egui::RichText::new(*label).small().weak());
        }
    });
}

fn chart_frame(ui: &mut egui::Ui, height: f32) -> egui::Rect {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::hover(),
    );
    ui.painter()
        .rect_filled(rect, 4.0, egui::Color32::from_rgb(30, 33, 40));
    rect
}

fn draw_stacked_days(
    ui: &mut egui::Ui,
    days: &[klep_core::DayReviews],
    max_days: usize,
    height: f32,
) {
    let window: Vec<_> = days.iter().rev().take(max_days).rev().cloned().collect();
    let max = window
        .iter()
        .map(klep_core::DayReviews::total)
        .max()
        .unwrap_or(1)
        .max(1);
    let rect = chart_frame(ui, height);
    if window.is_empty() {
        return;
    }
    let inner = rect.shrink(4.0);
    let width = inner.width() / window.len() as f32;
    for (i, day) in window.iter().enumerate() {
        let x = inner.left() + i as f32 * width;
        let mut y = inner.bottom();
        for (count, color) in [
            (day.filtered, FILTERED_COLOR),
            (day.mature, MATURE_COLOR),
            (day.young, YOUNG_COLOR),
            (day.relearn, RELEARN_COLOR),
            (day.learn, LEARN_COLOR),
        ] {
            let seg = count as f32 / max as f32 * inner.height();
            y -= seg;
            if seg > 0.0 {
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(x + 0.5, y),
                        egui::vec2((width - 1.0).max(1.0), seg),
                    ),
                    1.0,
                    color,
                );
            }
        }
    }
}

fn draw_day_counts(ui: &mut egui::Ui, counts: &[(i64, u32)], color: egui::Color32, height: f32) {
    let max = counts
        .iter()
        .map(|(_, count)| *count)
        .max()
        .unwrap_or(1)
        .max(1);
    let rect = chart_frame(ui, height);
    if counts.is_empty() {
        return;
    }
    let inner = rect.shrink(4.0);
    let width = inner.width() / counts.len() as f32;
    for (i, (_, count)) in counts.iter().enumerate() {
        let bar = *count as f32 / max as f32 * inner.height();
        if bar > 0.0 {
            let x = inner.left() + i as f32 * width;
            ui.painter().rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(x + 0.5, inner.bottom() - bar),
                    egui::vec2((width - 1.0).max(1.0), bar),
                ),
                1.0,
                color,
            );
        }
    }
}

fn draw_hourly(ui: &mut egui::Ui, hours: &[klep_core::HourBucket]) {
    let max = hours
        .iter()
        .map(|hour| hour.total)
        .max()
        .unwrap_or(1)
        .max(1);
    let rect = chart_frame(ui, 110.0);
    if hours.len() != 24 {
        ui.label("no hourly data");
        return;
    }
    let inner = rect.shrink(4.0);
    let width = inner.width() / 24.0;
    for (i, hour) in hours.iter().enumerate() {
        let bar = hour.total as f32 / max as f32 * inner.height();
        let x = inner.left() + i as f32 * width;
        if bar > 0.0 {
            ui.painter().rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(x + 0.5, inner.bottom() - bar),
                    egui::vec2((width - 1.0).max(1.0), bar),
                ),
                1.0,
                REVIEW_COLOR,
            );
        }
        if i % 3 == 0 {
            ui.painter().text(
                egui::pos2(x, rect.bottom() + 2.0),
                egui::Align2::LEFT_TOP,
                format!("{i}"),
                egui::FontId::proportional(10.0),
                egui::Color32::GRAY,
            );
        }
    }
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 14.0), egui::Sense::hover());
    let _ = rect;
}

fn draw_bar_list(ui: &mut egui::Ui, rows: &[(String, u32)], color: egui::Color32) {
    let max = rows
        .iter()
        .map(|(_, count)| *count)
        .max()
        .unwrap_or(1)
        .max(1);
    for (label, count) in rows {
        ui.horizontal(|ui| {
            ui.add_sized(
                [90.0, 18.0],
                egui::Label::new(egui::RichText::new(label).small()).truncate(),
            );
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width() - 60.0, 14.0),
                egui::Sense::hover(),
            );
            ui.painter()
                .rect_filled(rect, 2.0, egui::Color32::from_rgb(30, 33, 40));
            let bar = *count as f32 / max as f32 * rect.width();
            ui.painter().rect_filled(
                egui::Rect::from_min_size(rect.min, egui::vec2(bar, rect.height())),
                2.0,
                color,
            );
            ui.label(egui::RichText::new(count.to_string()).small().weak());
        });
    }
}

fn draw_graphs(ui: &mut egui::Ui, graphs: &klep_core::GraphsData) {
    ui.label(egui::RichText::new("today").strong());
    let today = &graphs.today;
    let minutes = today.answer_millis as f32 / 60_000.0;
    let accuracy = if today.answer_count == 0 {
        0.0
    } else {
        today.correct_count as f32 / today.answer_count as f32 * 100.0
    };
    ui.label(format!(
        "{} answers · {:.1} min · {:.0}% correct · {} learn / {} review / {} relearn",
        today.answer_count,
        minutes,
        accuracy,
        today.learn_count,
        today.review_count,
        today.relearn_count
    ));
    ui.add_space(6.0);

    let counts = &graphs.card_counts;
    ui.horizontal_wrapped(|ui| {
        for (label, count) in [
            ("new", counts.new),
            ("learn", counts.learn),
            ("relearn", counts.relearn),
            ("young", counts.young),
            ("mature", counts.mature),
            ("suspended", counts.suspended),
            ("buried", counts.buried),
        ] {
            ui.label(
                egui::RichText::new(format!("{label} {count}"))
                    .small()
                    .monospace(),
            );
        }
    });

    ui.add_space(10.0);
    ui.label(egui::RichText::new("reviews / day (30d)").strong());
    draw_stacked_days(ui, &graphs.reviews, 30, 120.0);
    legend(
        ui,
        &[
            ("learn", LEARN_COLOR),
            ("relearn", RELEARN_COLOR),
            ("young", YOUNG_COLOR),
            ("mature", MATURE_COLOR),
            ("filtered", FILTERED_COLOR),
        ],
    );

    ui.add_space(10.0);
    ui.label(egui::RichText::new("cards added / day (30d)").strong());
    let day_today = graphs.reviews.iter().map(|day| day.day).max().unwrap_or(0);
    let added: Vec<(i64, u32)> = (0..30)
        .rev()
        .map(|offset| {
            let day = day_today - offset;
            let count = graphs
                .added
                .iter()
                .find(|entry| entry.day == day)
                .map(|entry| entry.count)
                .unwrap_or(0);
            (day, count)
        })
        .collect();
    draw_day_counts(ui, &added, ADDED_COLOR, 80.0);

    ui.add_space(10.0);
    ui.label(egui::RichText::new("future due — next 30 days").strong());
    let due: Vec<(i64, u32)> = (0..30)
        .map(|offset| {
            let count = graphs
                .future_due
                .iter()
                .find(|entry| entry.day == offset)
                .map(|entry| entry.count)
                .unwrap_or(0);
            (offset, count)
        })
        .collect();
    draw_day_counts(ui, &due, REVIEW_COLOR, 80.0);
    ui.label(
        egui::RichText::new(format!("daily load: {} cards", graphs.daily_load))
            .small()
            .weak(),
    );

    ui.add_space(10.0);
    ui.label(egui::RichText::new("review intervals").strong());
    let mut interval_rows: Vec<(String, u32)> = Vec::new();
    let mut edges = [
        (1u32, "1d"),
        (2, "2d"),
        (3, "3d"),
        (4, "4d"),
        (5, "5d"),
        (6, "6d"),
        (7, "7d"),
        (8, "8-14d"),
        (15, "15-30d"),
        (31, "31-60d"),
        (61, "61-90d"),
        (91, "91-180d"),
        (181, "181-365d"),
        (366, "1y+"),
    ]
    .into_iter()
    .peekable();
    while let Some((start, label)) = edges.next() {
        let end = edges
            .peek()
            .map(|(next, _)| next.saturating_sub(1))
            .unwrap_or(u32::MAX);
        let count: u32 = graphs
            .intervals
            .iter()
            .filter(|bucket| bucket.days >= start && bucket.days <= end)
            .map(|bucket| bucket.count)
            .sum();
        if count > 0 {
            interval_rows.push((label.to_string(), count));
        }
    }
    if interval_rows.is_empty() {
        ui.label(egui::RichText::new("no reviews yet").small().weak());
    } else {
        draw_bar_list(ui, &interval_rows, MATURE_COLOR);
    }

    ui.add_space(10.0);
    ui.label(egui::RichText::new("ease").strong());
    let ease_rows: Vec<(String, u32)> = graphs
        .eases
        .iter()
        .map(|bucket| (format!("{}%", bucket.ease), bucket.count))
        .collect();
    if ease_rows.is_empty() {
        ui.label(egui::RichText::new("no reviews yet").small().weak());
    } else {
        ui.label(
            egui::RichText::new(format!("average: {:.0}%", graphs.average_ease))
                .small()
                .weak(),
        );
        draw_bar_list(ui, &ease_rows, YOUNG_COLOR);
    }

    ui.add_space(10.0);
    ui.label(egui::RichText::new("answer buttons (all time)").strong());
    if graphs.buttons.is_empty() {
        ui.label(egui::RichText::new("no reviews yet").small().weak());
    } else {
        let max = graphs
            .buttons
            .iter()
            .map(|row| row.learning + row.young + row.mature)
            .max()
            .unwrap_or(1)
            .max(1);
        for row in &graphs.buttons {
            let label = match row.button {
                1 => "again",
                2 => "hard",
                3 => "good",
                _ => "easy",
            };
            let total = row.learning + row.young + row.mature;
            ui.horizontal(|ui| {
                ui.add_sized(
                    [50.0, 16.0],
                    egui::Label::new(egui::RichText::new(label).small()),
                );
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width() - 40.0, 14.0),
                    egui::Sense::hover(),
                );
                ui.painter()
                    .rect_filled(rect, 2.0, egui::Color32::from_rgb(30, 33, 40));
                let mut x = rect.left();
                for (count, color) in [
                    (row.learning, LEARN_COLOR),
                    (row.young, YOUNG_COLOR),
                    (row.mature, MATURE_COLOR),
                ] {
                    let width = count as f32 / max as f32 * rect.width();
                    if width > 0.0 {
                        ui.painter().rect_filled(
                            egui::Rect::from_min_size(
                                egui::pos2(x, rect.top()),
                                egui::vec2(width, rect.height()),
                            ),
                            2.0,
                            color,
                        );
                        x += width;
                    }
                }
                ui.label(egui::RichText::new(total.to_string()).small().weak());
            });
        }
    }

    ui.add_space(10.0);
    ui.label(egui::RichText::new("hourly reviews (all time)").strong());
    draw_hourly(ui, &graphs.hours);

    ui.add_space(10.0);
    ui.label(egui::RichText::new("true retention (all time)").strong());
    let retention = &graphs.true_retention;
    let young_total = retention.young_passed + retention.young_failed;
    let mature_total = retention.mature_passed + retention.mature_failed;
    let pct = |passed: u32, total: u32| {
        if total == 0 {
            "—".to_string()
        } else {
            format!("{:.0}%", passed as f32 / total as f32 * 100.0)
        }
    };
    ui.label(format!(
        "young {} ({} reviews) · mature {} ({} reviews)",
        pct(retention.young_passed, young_total),
        young_total,
        pct(retention.mature_passed, mature_total),
        mature_total
    ));
    if graphs.fsrs {
        ui.label(format!(
            "FSRS · retrievability {:.1}%",
            graphs.retrievability * 100.0
        ));
    }
    ui.label(
        egui::RichText::new(format!("day rolls over at {}:00", graphs.rollover_hour))
            .small()
            .weak(),
    );
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("klepci")
            .with_inner_size([720.0, 860.0])
            .with_min_inner_size([420.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "klepci",
        options,
        Box::new(|cc| {
            cc.egui_ctx.global_style_mut(|style| {
                style.text_styles.insert(
                    egui::TextStyle::Body,
                    egui::FontId::new(16.0, egui::FontFamily::Proportional),
                );
                style.text_styles.insert(
                    egui::TextStyle::Button,
                    egui::FontId::new(16.0, egui::FontFamily::Proportional),
                );
                style.text_styles.insert(
                    egui::TextStyle::Heading,
                    egui::FontId::new(22.0, egui::FontFamily::Proportional),
                );
            });
            let mut visuals = egui::Visuals::dark();
            visuals.selection.bg_fill = egui::Color32::from_rgb(122, 162, 247);
            cc.egui_ctx.set_visuals(visuals);
            Ok(Box::new(KlepciApp::new()))
        }),
    )
}

#[cfg(test)]
mod test {
    use klep_core::html_to_text;

    #[test]
    fn strips_tags_and_entities() {
        assert_eq!(
            html_to_text("<div>Hello<br>w&nbsp;orld &amp; <b>bye</b></div>"),
            "Hello\nw orld & bye"
        );
    }

    #[test]
    fn placeholders_for_media() {
        assert_eq!(
            html_to_text(r#"a<img src="x.jpg">b[sound:y.mp3]"#),
            "a[image]b🔊"
        );
    }
}
