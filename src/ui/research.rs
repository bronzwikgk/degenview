//! Dedicated Research Runner tab.

use crate::lock::lock;
use crate::model::research::{
    ExecutionModel, HistoricalConfig, HistoricalTrade, UniverseMember,
    close_above_previous_high_entries, merge_universes, simulate_long,
};
use crate::model::{DataSource, Timeframe};
use crate::service::{
    fetch_klines,
    work_queue::{self, Pending},
};
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
    #[serde(default)]
    reports: Vec<RunReport>,
}
#[derive(Clone, Default, Deserialize, Serialize)]
struct RunReport {
    scanned: usize,
    skipped: usize,
    trades: Vec<ResearchTrade>,
}
#[derive(Clone, Deserialize, Serialize)]
struct ResearchTrade {
    symbol: String,
    universe_labels: Vec<String>,
    trade: HistoricalTrade,
}
impl RunReport {
    fn net_return(&self) -> f64 {
        self.trades.iter().map(|row| row.trade.net_return_pct).sum()
    }
    fn wins(&self) -> usize {
        self.trades
            .iter()
            .filter(|row| row.trade.net_return_pct > 0.0)
            .count()
    }
}
fn run_local(members: Vec<UniverseMember>, execution: ExecutionModel) -> RunReport {
    let mut report = RunReport::default();
    for member in members {
        if member.source != "Local" {
            report.skipped += 1;
            continue;
        }
        match fetch_klines(DataSource::Local, &member.symbol, Timeframe::DAY_1, 1500) {
            Ok(candles) if candles.len() > 2 => {
                report.scanned += 1;
                let entries = close_above_previous_high_entries(&candles);
                let config = HistoricalConfig {
                    execution,
                    stop_loss_pct: 5.0,
                    target_pct: 10.0,
                    max_holding_bars: 45,
                    commission_pct: 0.0,
                    slippage_pct: 0.0,
                };
                report
                    .trades
                    .extend(
                        simulate_long(&candles, &entries, config)
                            .into_iter()
                            .map(|trade| ResearchTrade {
                                symbol: member.symbol.clone(),
                                universe_labels: member.universe_labels.clone(),
                                trade,
                            }),
                    );
            }
            _ => report.skipped += 1,
        }
    }
    report
}
pub struct ResearchTab {
    id: u64,
    section: Section,
    loaded: bool,
    saved: SavedResearch,
    notice: Option<String>,
    run: Option<Pending<RunReport>>,
    report: Option<RunReport>,
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
            run: None,
            report: None,
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
                    self.report = self.saved.reports.last().cloned();
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
        if let Some(run) = &mut self.run {
            if let Some(report) = run.poll() {
                self.saved.reports.push(report.clone());
                self.report = Some(report);
                if let Ok(s) = store.lock() {
                    if let Err(error) = s.research_set(KEY, &self.saved) {
                        self.notice =
                            Some(format!("Completed, but could not save the report: {error}"));
                    } else {
                        self.notice = Some("Completed and saved locally.".into());
                    }
                }
            }
            if !run.is_pending() {
                self.run = None;
            }
        }
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
                if ui
                    .add_enabled(
                        self.run.is_none(),
                        egui::Button::new("Run Local NSE historical test"),
                    )
                    .clicked()
                {
                    let names: Vec<String> = self
                        .saved
                        .universe_set
                        .lines()
                        .map(str::trim)
                        .filter(|name| !name.is_empty())
                        .map(str::to_owned)
                        .collect();
                    let sets: Vec<(String, Vec<(String, String)>)> = lock(watchlist)
                        .lists()
                        .iter()
                        .filter(|list| names.iter().any(|name| name == &list.name))
                        .map(|list| {
                            (
                                list.name.clone(),
                                list.instruments()
                                    .iter()
                                    .filter(|item| item.instrument.source == DataSource::Local)
                                    .map(|item| ("Local".into(), item.instrument.api_symbol()))
                                    .collect(),
                            )
                        })
                        .collect();
                    let members = merge_universes(sets);
                    if members.is_empty() {
                        self.notice =
                            Some("Select a Local NSE Watchlist in Universe first.".into());
                    } else {
                        let execution = if self.saved.execution == "Signal close" {
                            ExecutionModel::SignalClose
                        } else {
                            ExecutionModel::NextBarOpen
                        };
                        self.report = None;
                        self.run = Some(
                            work_queue::RESEARCH
                                .start(Some(ui.ctx()), move || run_local(members, execution)),
                        );
                    }
                }
                if self.run.is_some() {
                    ui.spinner();
                    ui.label("Running historical test...");
                }
            }
            Section::Reports => {
                if let Some(report) = &self.report {
                    let total = report.trades.len();
                    let win_rate = if total == 0 {
                        0.0
                    } else {
                        report.wins() as f64 / total as f64 * 100.0
                    };
                    ui.label(format!(
                        "{} symbols scanned · {} skipped · {} trades",
                        report.scanned, report.skipped, total
                    ));
                    ui.label(format!(
                        "Net return (one unit per trade): {:+.2}% · Win rate: {:.1}%",
                        report.net_return(),
                        win_rate
                    ));
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for row in &report.trades {
                            ui.label(format!(
                                "{} · {:.2}% · {:?} · {}",
                                row.symbol,
                                row.trade.net_return_pct,
                                row.trade.reason,
                                row.universe_labels.join(", ")
                            ));
                        }
                    });
                } else {
                    ui.label("No completed Research Runner runs yet.");
                }
            }
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
