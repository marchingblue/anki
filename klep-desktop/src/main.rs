//! klepci desktop: minimalist egui frontend over [`klep_core::Core`].
//!
//! Three tabs, nothing else: Decks, Review, Stats. Review is text-only in
//! v1 (images show as placeholders); media rendering lands in v1.1.

use std::path::PathBuf;
use std::time::Instant;

use eframe::egui;
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

/// Card HTML to readable plain text. Images/audio become placeholders
/// (media rendering is v1.1).
fn html_to_text(html: &str) -> String {
    let mut out = html
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("</p>", "\n")
        .replace("</div>", "\n")
        .replace("</li>", "\n")
        .replace("<li>", "\n• ");
    // [sound:name] -> placeholder
    while let Some(start) = out.find("[sound:") {
        let Some(end) = out[start..].find(']') else {
            break;
        };
        out.replace_range(start..start + end + 1, "🔊");
    }
    // <img ...> -> placeholder
    while let Some(start) = out.find("<img") {
        let Some(end) = out[start..].find('>') else {
            break;
        };
        out.replace_range(start..start + end + 1, "[image]");
    }
    // strip remaining tags
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

enum KeyAction {
    Show,
    Answer(u8),
}

struct KlepciApp {
    core: Option<Core>,
    error: Option<String>,
    tab: Tab,
    decks: Vec<DeckInfo>,
    card: Option<CardView>,
    answer: Option<AnswerView>,
    shown_at: Option<Instant>,
    done_message: Option<String>,
    stats: Option<serde_json::Value>,
}

impl KlepciApp {
    fn new() -> Self {
        let mut app = KlepciApp {
            core: None,
            error: None,
            tab: Tab::Decks,
            decks: vec![],
            card: None,
            answer: None,
            shown_at: None,
            done_message: None,
            stats: None,
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
    }

    fn next(&mut self) {
        self.card = None;
        self.answer = None;
        self.shown_at = None;
        self.done_message = None;
        let next = self.core.as_mut().map(|core| core.next_card());
        match next {
            Some(result) => match self.fail(result) {
                Some(Some(card)) => {
                    self.card = Some(card);
                    self.shown_at = Some(Instant::now());
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
                }
            }
            None => self.error = Some("no collection open".to_string()),
        }
    }

    fn load_stats(&mut self) {
        let json = self.core.as_mut().map(|core| core.timing_json("", 10));
        match json {
            Some(result) => self.stats = self.fail(result).and_then(|text| serde_json::from_str(&text).ok()),
            None => {
                self.error = Some("no collection open".to_string());
                self.stats = None;
            }
        }
    }
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
            Some(KeyAction::Answer(rating)) => self.answer(rating),
            None => {}
        }

        egui::Panel::top("tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                for (tab, label) in [
                    (Tab::Decks, "Decks"),
                    (Tab::Review, "Review"),
                    (Tab::Stats, "Stats"),
                ] {
                    if ui.selectable_label(self.tab == tab, label).clicked() {
                        self.tab = tab;
                        if tab == Tab::Review && self.card.is_none() {
                            self.next();
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
                Tab::Stats => self.view_stats(ui),
            }
        });
    }
}

impl KlepciApp {
    fn view_decks(&mut self, ui: &mut egui::Ui) {
        if self.decks.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label("no decks yet — import an .apkg in Anki first");
            });
            return;
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for deck in self.decks.clone() {
                if ui
                    .add_sized([ui.available_width(), 44.0], egui::Button::new(&deck.name))
                    .clicked()
                {
                    self.tab = Tab::Review;
                    self.next();
                }
            }
        });
    }

    fn view_review(&mut self, ui: &mut egui::Ui) {
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
    }

    fn view_stats(&mut self, ui: &mut egui::Ui) {
        let Some(stats) = self.stats.clone() else {
            ui.centered_and_justified(|ui| {
                ui.label("no stats yet — review some cards first");
            });
            return;
        };
        let reviews = stats["reviews"].as_u64().unwrap_or(0);
        let mean = stats["mean_secs"].as_f64().unwrap_or(0.0);
        let total = stats["total_secs"].as_f64().unwrap_or(0.0);
        ui.heading(format!("{reviews} reviews · mean {mean:.1}s · total {total:.0}s"));
        ui.add_space(8.0);
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
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(weeks as f32 * cell, 7.0 * cell), egui::Sense::hover());
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
    }
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
    use super::html_to_text;

    #[test]
    fn strips_tags_and_entities() {
        assert_eq!(
            html_to_text("<div>Hello<br>w&nbsp;orld &amp; <b>bye</b></div>"),
            "Hello\nw orld & bye"
        );
    }

    #[test]
    fn placeholders_for_media() {
        assert_eq!(html_to_text(r#"a<img src="x.jpg">b[sound:y.mp3]"#), "a[image]b🔊");
    }
}
