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
use std::collections::BTreeMap;
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
    #[serde(default = "default_stop_loss")]
    stop_loss_pct: f64,
    #[serde(default = "default_target")]
    target_pct: f64,
    #[serde(default = "default_max_holding")]
    max_holding_bars: usize,
    #[serde(default)]
    commission_pct: f64,
    #[serde(default)]
    slippage_pct: f64,
    #[serde(default)]
    reports: Vec<RunReport>,
}
const fn default_stop_loss() -> f64 {
    5.0
}
const fn default_target() -> f64 {
    10.0
}
const fn default_max_holding() -> usize {
    45
}

/// Converts an ISO calendar day to UTC midnight without relying on the host locale/timezone.
fn parse_day(text: &str) -> Result<Option<i64>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let mut fields = text.split('-');
    let year = fields.next().and_then(|value| value.parse::<i64>().ok());
    let month = fields.next().and_then(|value| value.parse::<i64>().ok());
    let day = fields.next().and_then(|value| value.parse::<i64>().ok());
    if fields.next().is_some()
        || !matches!((year, month, day), (Some(_), Some(1..=12), Some(1..=31)))
    {
        return Err("Dates must use YYYY-MM-DD.".into());
    }
    let (mut year, month, day) = (year.unwrap(), month.unwrap(), day.unwrap());
    year -= i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month_offset = if month > 2 { -3 } else { 9 };
    let doy = (153 * (month + month_offset) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Ok(Some((era * 146_097 + doe - 719_468) * 86_400_000))
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
    fn by_universe(&self) -> BTreeMap<&str, (usize, f64)> {
        let mut result = BTreeMap::new();
        for row in &self.trades {
            for label in &row.universe_labels {
                let entry = result.entry(label.as_str()).or_insert((0, 0.0));
                entry.0 += 1;
                entry.1 += row.trade.net_return_pct;
            }
        }
        result
    }
}
fn run_local(
    members: Vec<UniverseMember>,
    start_ms: Option<i64>,
    end_ms: Option<i64>,
    config: HistoricalConfig,
) -> RunReport {
    let mut report = RunReport::default();
    for member in members {
        if member.source != "Local" {
            report.skipped += 1;
            continue;
        }
        match fetch_klines(DataSource::Local, &member.symbol, Timeframe::DAY_1, 1500) {
            Ok(candles) => {
                let candles: Vec<_> = candles
                    .into_iter()
                    .filter(|bar| start_ms.is_none_or(|start| bar.open_time >= start))
                    .filter(|bar| end_ms.is_none_or(|end| bar.open_time <= end))
                    .collect();
                if candles.len() <= 2 {
                    report.skipped += 1;
                    continue;
                }
                report.scanned += 1;
                let entries = close_above_previous_high_entries(&candles);
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
            Err(_) => report.skipped += 1,
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
                stop_loss_pct: default_stop_loss(),
                target_pct: default_target(),
                max_holding_bars: default_max_holding(),
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
                    ui.label("Start (YYYY-MM-DD)");
                    ui.text_edit_singleline(&mut self.saved.start);
                    ui.label("End (YYYY-MM-DD)");
                    ui.text_edit_singleline(&mut self.saved.end);
                });
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut self.saved.stop_loss_pct).suffix("% stop"));
                    ui.add(egui::DragValue::new(&mut self.saved.target_pct).suffix("% target"));
                    ui.add(
                        egui::DragValue::new(&mut self.saved.max_holding_bars).suffix(" bars max"),
                    );
                    ui.add(
                        egui::DragValue::new(&mut self.saved.commission_pct).suffix("% commission"),
                    );
                    ui.add(egui::DragValue::new(&mut self.saved.slippage_pct).suffix("% slippage"));
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
                    let start = match parse_day(&self.saved.start) {
                        Ok(value) => value,
                        Err(error) => {
                            self.notice = Some(error);
                            return;
                        }
                    };
                    let end = match parse_day(&self.saved.end) {
                        Ok(value) => value,
                        Err(error) => {
                            self.notice = Some(error);
                            return;
                        }
                    };
                    if start.zip(end).is_some_and(|(start, end)| start > end) {
                        self.notice = Some("Start date must not be after end date.".into());
                        return;
                    }
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
                        let config = HistoricalConfig {
                            execution,
                            stop_loss_pct: self.saved.stop_loss_pct,
                            target_pct: self.saved.target_pct,
                            max_holding_bars: self.saved.max_holding_bars,
                            commission_pct: self.saved.commission_pct,
                            slippage_pct: self.saved.slippage_pct,
                        };
                        self.report = None;
                        self.run = Some(work_queue::RESEARCH.start(Some(ui.ctx()), move || {
                            run_local(members, start, end, config)
                        }));
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
                    if report.by_universe().len() > 1 {
                        ui.separator();
                        ui.strong("Per source universe");
                        for (label, (trades, net_return)) in report.by_universe() {
                            ui.label(format!("{label}: {trades} trades · {net_return:+.2}%"));
                        }
                    }
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
