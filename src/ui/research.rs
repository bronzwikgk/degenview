//! Dedicated Research Runner tab.

use crate::lock::lock;
use crate::store::{SharedStore, watchlist::WatchlistStore};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

const KEY: &str = "workspace";

#[derive(Clone, Copy, Default, Deserialize, Eq, PartialEq, Serialize)]
enum Section {
    #[default]
    Universe,
    Strategy,
    HistoricalTest,
    Reports,
}
impl Section {
    const ALL: [Self; 4] = [
        Self::Universe,
        Self::Strategy,
        Self::HistoricalTest,
        Self::Reports,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::Universe => "Universe",
            Self::Strategy => "Strategy",
            Self::HistoricalTest => "Historical Test",
            Self::Reports => "Reports",
        }
    }
}
#[derive(Clone, Default, Deserialize, Serialize)]
struct SavedResearch {
    universe_set: String,
    strategy_name: String,
    context_signal: String,
    context_trigger: String,
    context_confirmation: String,
    setup_signal: String,
    setup_trigger: String,
    setup_confirmation: String,
    entry_signal: String,
    entry_trigger: String,
    entry_confirmation: String,
    start: String,
    end: String,
    execution: String,
}
pub struct ResearchTab {
    id: u64,
    section: Section,
    loaded: bool,
    saved: SavedResearch,
    notice: Option<String>,
}
impl ResearchTab {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            section: Section::default(),
            loaded: false,
            saved: SavedResearch {
                execution: "Next bar open".into(),
                ..Default::default()
            },
            notice: None,
        }
    }
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn title(&self) -> &str {
        "Research"
    }
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        store: &SharedStore,
        watchlist: &Arc<Mutex<WatchlistStore>>,
    ) {
        if !self.loaded {
            self.loaded = true;
            if let Ok(s) = store.lock() {
                if let Some(v) = s.research_get(KEY) {
                    self.saved = v;
                }
            }
        }
        ui.horizontal(|ui| {
            ui.heading("Research Runner");
            for s in Section::ALL {
                ui.selectable_value(&mut self.section, s, s.label());
            }
        });
        ui.separator();
        match self.section {
            Section::Universe => {
                ui.label("Universe Set");
                ui.text_edit_multiline(&mut self.saved.universe_set);
                ui.small("Selected Watchlists and frozen screener snapshots. One line per source universe.");
                ui.separator();
                ui.label("Add Watchlist to Universe Set");
                let lists: Vec<String> = lock(watchlist)
                    .lists()
                    .iter()
                    .map(|list| list.name.clone())
                    .collect();
                for name in lists {
                    if ui.button(&name).clicked()
                        && !self
                            .saved
                            .universe_set
                            .lines()
                            .any(|line| line.trim() == name)
                    {
                        if !self.saved.universe_set.is_empty() {
                            self.saved.universe_set.push('\n');
                        }
                        self.saved.universe_set.push_str(&name);
                    }
                }
            }
            Section::Strategy => {
                ui.label("Strategy name");
                ui.text_edit_singleline(&mut self.saved.strategy_name);
                for (name, value) in [
                    ("Context signal", &mut self.saved.context_signal),
                    ("Context trigger", &mut self.saved.context_trigger),
                    ("Context confirmation", &mut self.saved.context_confirmation),
                    ("Setup signal", &mut self.saved.setup_signal),
                    ("Setup trigger", &mut self.saved.setup_trigger),
                    ("Setup confirmation", &mut self.saved.setup_confirmation),
                    ("Entry signal", &mut self.saved.entry_signal),
                    ("Entry trigger", &mut self.saved.entry_trigger),
                    ("Entry confirmation", &mut self.saved.entry_confirmation),
                ] {
                    ui.label(name);
                    ui.text_edit_singleline(value);
                }
            }
            Section::HistoricalTest => {
                ui.horizontal(|ui| {
                    ui.label("Start");
                    ui.text_edit_singleline(&mut self.saved.start);
                    ui.label("End");
                    ui.text_edit_singleline(&mut self.saved.end);
                });
                egui::ComboBox::from_id_salt("research-execution")
                    .selected_text(&self.saved.execution)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.saved.execution,
                            "Signal close".into(),
                            "Signal close",
                        );
                        ui.selectable_value(
                            &mut self.saved.execution,
                            "Next bar open".into(),
                            "Next bar open",
                        );
                    });
                ui.add_enabled(
                    false,
                    egui::Button::new("Run historical test (engine integration pending)"),
                );
            }
            Section::Reports => ui.label("No completed Research Runner runs yet."),
        };
        if ui.button("Save research workspace").clicked() {
            match store
                .lock()
                .map_err(|e| e.to_string())
                .and_then(|s| s.research_set(KEY, &self.saved).map_err(|e| e.to_string()))
            {
                Ok(_) => self.notice = Some("Saved locally.".into()),
                Err(e) => self.notice = Some(e),
            }
        }
        if let Some(n) = &self.notice {
            ui.small(n);
        }
    }
}
