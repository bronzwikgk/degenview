//! The watchlist sidebar: drawing it and acting on what it reports.

use super::ChartWorkspace;
use super::alerts::AlertRequest;
use crate::lock::lock;
use crate::model::InstrumentId;
use crate::model::alert::AlertAsset;
use crate::model::watchlist::text::ImportReport;
use crate::model::watchlist::{EntryId, WatchlistError, WatchlistInstrument, WatchlistNotice};
use crate::model::{DataSource, Timeframe};
use crate::service::market::trend;
use crate::service::system::file_dialog;
use crate::service::work_queue::{self, Pending};
use crate::ui::watchlist::sidebar::{
    self, WatchlistAction, WatchlistSidebarInput, WatchlistSidebarState,
};
use eframe::egui;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

const SCREENER_QUERIES_KEY: &str = "screener.local_nse.queries";
const TURNOVER_TEMPLATE_NAME: &str = "Turnover: 3M + Current + 6W";
const TURNOVER_TEMPLATE: &str = "month_turnover(0) > 200000000 AND month_turnover(1) > 100000000 AND month_turnover(2) > 100000000 AND month_turnover(3) > 100000000 AND avg_week_turnover(6) > 50000000";

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
struct SavedScreenerQuery {
    name: String,
    text: String,
}

#[derive(Clone, Debug, Default)]
struct ScanReport {
    matches: Vec<ScanMatch>,
    scanned: usize,
    skipped: usize,
    cancelled: bool,
    error: Option<String>,
}

#[derive(Clone, Debug)]
struct ScanMatch {
    symbol: String,
    close: f64,
    return_5: Option<f64>,
    rsi_14: Option<f64>,
    volume: f64,
}

#[derive(Clone, Copy, Default, PartialEq)]
enum ScreenerSort {
    #[default]
    Symbol,
    Close,
    Return5,
    Rsi14,
    Volume,
}

#[derive(Default)]
struct ScanProgress {
    scanned: AtomicUsize,
    total: AtomicUsize,
    cancelled: AtomicBool,
}

fn scan_local_nse(query: String, progress: Arc<ScanProgress>) -> ScanReport {
    let expression = match crate::model::screener::parse(&query) {
        Ok(value) => value,
        Err(error) => {
            return ScanReport {
                error: Some(error.to_string()),
                ..Default::default()
            };
        }
    };
    let symbols = match crate::service::market::local_csv::symbols() {
        Ok(value) => value,
        Err(error) => {
            return ScanReport {
                error: Some(error.to_string()),
                ..Default::default()
            };
        }
    };
    progress.total.store(symbols.len(), Ordering::Relaxed);
    let bars_needed = crate::model::screener::required_bars(&expression).max(2) as u32;
    let mut report = ScanReport::default();
    for symbol in symbols {
        if progress.cancelled.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }
        match crate::service::fetch_klines(
            DataSource::Local,
            &symbol,
            Timeframe::DAY_1,
            bars_needed,
        ) {
            Ok(bars) if crate::model::screener::matches(&expression, &bars) => {
                report.scanned += 1;
                let last = bars.last().expect("matched candles have a latest bar");
                report.matches.push(ScanMatch {
                    symbol,
                    close: last.close,
                    return_5: crate::model::screener::value(
                        &crate::model::screener::Field::Return(5),
                        &bars,
                    ),
                    rsi_14: crate::model::screener::value(
                        &crate::model::screener::Field::Rsi(14),
                        &bars,
                    ),
                    volume: last.volume,
                });
            }
            Ok(_) => report.scanned += 1,
            Err(_) => report.skipped += 1,
        }
        progress.scanned.fetch_add(1, Ordering::Relaxed);
    }
    report
}

/// Shows why a change was refused. A failed save already shows the store's own message.
fn report_watchlist(slot: &mut Option<WatchlistNotice>, result: Result<(), WatchlistError>) {
    match result {
        Ok(()) | Err(WatchlistError::SaveFailed) => {}
        Err(error) => *slot = Some(error.notice()),
    }
}

/// Asks for a file of watchlist symbols and reads it, on the workers that wait on the user. The
/// answer is `None` when the user cancels.
fn read_import_file(ctx: &egui::Context) -> Pending<Result<Option<String>, String>> {
    work_queue::SNAPSHOTS.start(Some(ctx), || {
        file_dialog::choose_csv("Choose watchlist symbols").and_then(|path| {
            path.map(|path| {
                file_dialog::read_text_up_to(&path, crate::model::watchlist::text::MAX_IMPORT_BYTES)
            })
            .transpose()
        })
    })
}

/// Asks where to save the watchlist `name` and writes `text` there, on the workers that wait on
/// the user.
fn export_to_file(ctx: &egui::Context, name: &str, text: String) -> Pending<Result<(), String>> {
    let file_name = crate::model::watchlist::text::export_file_name(name);
    work_queue::SNAPSHOTS.start(Some(ctx), move || {
        file_dialog::choose_save_path("Export watchlist", &file_name)
            .and_then(|path| {
                path.map(|path| file_dialog::write_text(&path, &text))
                    .transpose()
            })
            .map(|_| ())
    })
}

/// The watchlist sidebar's side of a tab: whether it shows, its own view state, what it last had
/// to say, and the requests it hands to the rest of the tab (a list for the symbol picker to add
/// to, a market to open in a new tab).
#[derive(Default)]
pub(super) struct WatchlistPane {
    /// Saved with the tab and its layouts.
    visible: bool,
    sidebar: WatchlistSidebarState,
    import_report: Option<ImportReport>,
    /// Why the last change was refused, until dismissed.
    notice: Option<WatchlistNotice>,
    /// The export being written.
    file_result: Option<Pending<Result<(), String>>>,
    /// The list a list's plus opened the symbol picker for; a pick is added to it.
    picker_target: Option<EntryId>,
    /// A row asked to open in a new tab; the app opens it after the frame.
    open_in_new_tab: Option<WatchlistInstrument>,
    screener_visible: bool,
    screener_query: String,
    screener_name: String,
    saved_screens: Vec<SavedScreenerQuery>,
    screens_loaded: bool,
    scan: Option<Pending<ScanReport>>,
    scan_progress: Option<Arc<ScanProgress>>,
    scan_report: Option<ScanReport>,
    screener_notice: Option<String>,
    screener_sort: ScreenerSort,
    screener_descending: bool,
    builder_field: usize,
    builder_operator: usize,
    builder_right: String,
}

impl WatchlistPane {
    pub(super) fn is_visible(&self) -> bool {
        self.visible
    }

    pub(super) fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    pub(super) fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Shows why a change was refused (a failed save shows the store's own message).
    pub(super) fn report(&mut self, result: Result<(), WatchlistError>) {
        report_watchlist(&mut self.notice, result);
    }

    /// The list the open symbol picker adds to, if it was opened for one.
    pub(super) fn picker_target(&self) -> Option<EntryId> {
        self.picker_target
    }

    /// The picker closed: a list is only its target while it is open.
    pub(super) fn clear_picker_target(&mut self) {
        self.picker_target = None;
    }

    #[cfg(test)]
    pub(super) fn set_picker_target(&mut self, list: Option<EntryId>) {
        self.picker_target = list;
    }

    /// The market a row asked to open in a new tab, once.
    pub(super) fn take_new_tab_request(&mut self) -> Option<WatchlistInstrument> {
        self.open_in_new_tab.take()
    }
}

impl ChartWorkspace {
    /// The sidebar at the right while it is shown; hidden, its quote demand stops.
    pub(super) fn show_watchlist_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if !self.watchlist_pane.visible {
            self.suspend_watchlist_demand();
            return;
        }
        let minimum_width = self
            .watchlist
            .lock()
            .ok()
            .map(|store| self.watchlist_pane.sidebar.minimum_width(store.lists()))
            .unwrap_or(286.0);
        egui::Panel::right("watchlist")
            .default_size(minimum_width.max(270.0))
            .min_size(minimum_width)
            .max_size(640.0)
            .frame(
                egui::Frame::new()
                    .fill(crate::ui::theme::chrome())
                    .inner_margin(egui::Margin::ZERO),
            )
            .show(ui, |ui| self.show_watchlist(ui, ctx));
    }

    fn show_watchlist(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.load_screener_queries();
        if self.watchlist_pane.screener_visible {
            self.show_screener(ui, ctx);
            return;
        }
        if let Some(export) = &mut self.watchlist_pane.file_result {
            if let Some(Err(error)) = export.poll() {
                self.watchlist_pane.notice = Some(WatchlistNotice::new("File Error", error));
            }
            if !export.is_pending() {
                self.watchlist_pane.file_result = None;
            }
        }
        let actions = {
            let watchlist = Arc::clone(&self.watchlist);
            let store = lock(&watchlist);
            let lists = store.lists();
            let flags = store.flags();
            let quote = |id: &InstrumentId| self.quote_book.get(id);
            let trends = trend::month().snapshot();
            let focused_market = self
                .resolved_focused_id()
                .and_then(|id| self.charts.iter().find(|chart| chart.id == id))
                .map(|chart| InstrumentId::new(chart.source, &chart.symbol));
            let store_notice = store
                .last_error()
                .map(|message| WatchlistNotice::new("Couldn\u{2019}t Save", message));
            let input = WatchlistSidebarInput {
                lists,
                flags,
                quote: &quote,
                trends: &trends,
                focused_market: focused_market.as_ref(),
                load_failed: store.load_failed(),
                error: self
                    .watchlist_pane
                    .notice
                    .as_ref()
                    .or(store_notice.as_ref()),
                import_report: self.watchlist_pane.import_report.as_ref(),
            };
            sidebar::show(ui, &mut self.watchlist_pane.sidebar, &input)
        };
        // Only the rows on screen ask for prices, not every list in the library.
        self.quote_coordinator.set_sidebar_demand(
            self.id,
            self.watchlist_pane.sidebar.quote_demand().to_vec(),
            true,
        );
        let repaint = ctx.clone();
        trend::month().request(
            self.watchlist_pane.sidebar.trend_demand(),
            Arc::new(move || repaint.request_repaint()),
        );
        self.apply_watchlist_actions(actions, ctx);
    }

    fn load_screener_queries(&mut self) {
        if self.watchlist_pane.screens_loaded {
            return;
        }
        self.watchlist_pane.screens_loaded = true;
        self.watchlist_pane.saved_screens = lock(&self.store)
            .setting(SCREENER_QUERIES_KEY)
            .unwrap_or_default();
        if !self
            .watchlist_pane
            .saved_screens
            .iter()
            .any(|query| query.name == TURNOVER_TEMPLATE_NAME)
        {
            self.watchlist_pane.saved_screens.push(SavedScreenerQuery {
                name: TURNOVER_TEMPLATE_NAME.to_owned(),
                text: TURNOVER_TEMPLATE.to_owned(),
            });
            self.persist_screener_queries();
        }
    }

    fn persist_screener_queries(&mut self) {
        if let Err(error) =
            lock(&self.store).set_setting(SCREENER_QUERIES_KEY, &self.watchlist_pane.saved_screens)
        {
            self.watchlist_pane.screener_notice = Some(format!("Could not save query: {error}"));
        }
    }

    fn show_screener(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if let Some(job) = &mut self.watchlist_pane.scan {
            if let Some(report) = job.poll() {
                self.watchlist_pane.scan_report = Some(report);
            }
            if !job.is_pending() {
                self.watchlist_pane.scan = None;
                self.watchlist_pane.scan_progress = None;
            }
        }
        ui.horizontal(|ui| {
            if ui.button("‹ Watchlists").clicked() {
                self.watchlist_pane.screener_visible = false;
            }
            ui.heading("Local NSE Screener");
        });
        ui.separator();
        ui.label("Condition (daily local data)");
        ui.add(
            egui::TextEdit::multiline(&mut self.watchlist_pane.screener_query)
                .hint_text("close > ema(20) AND rsi(14) > 50 AND return(5) >= 3")
                .desired_rows(3),
        );
        ui.small("Fields: open, high, low, close, volume, return(n), sma(n), ema(n), rsi(n). Join conditions with AND.");
        const FIELDS: [&str; 11] = [
            "close",
            "open",
            "high",
            "low",
            "volume",
            "return(5)",
            "sma(20)",
            "ema(20)",
            "rsi(14)",
            "month_turnover(0)",
            "avg_week_turnover(6)",
        ];
        const OPERATORS: [&str; 5] = [">", ">=", "<", "<=", "="];
        ui.horizontal(|ui| {
            ui.label("Add condition");
            egui::ComboBox::from_id_salt("screener-builder-field")
                .selected_text(FIELDS[self.watchlist_pane.builder_field])
                .show_ui(ui, |ui| {
                    for (index, field) in FIELDS.iter().enumerate() {
                        ui.selectable_value(&mut self.watchlist_pane.builder_field, index, *field);
                    }
                });
            egui::ComboBox::from_id_salt("screener-builder-operator")
                .selected_text(OPERATORS[self.watchlist_pane.builder_operator])
                .show_ui(ui, |ui| {
                    for (index, operator) in OPERATORS.iter().enumerate() {
                        ui.selectable_value(
                            &mut self.watchlist_pane.builder_operator,
                            index,
                            *operator,
                        );
                    }
                });
            ui.add(
                egui::TextEdit::singleline(&mut self.watchlist_pane.builder_right)
                    .hint_text("50 or ema(20)"),
            );
            if ui.button("Add").clicked() {
                let right = self.watchlist_pane.builder_right.trim();
                if right.is_empty() {
                    self.watchlist_pane.screener_notice =
                        Some("Enter a numeric value or another field.".to_owned());
                } else {
                    let clause = format!(
                        "{} {} {}",
                        FIELDS[self.watchlist_pane.builder_field],
                        OPERATORS[self.watchlist_pane.builder_operator],
                        right
                    );
                    if self.watchlist_pane.screener_query.trim().is_empty() {
                        self.watchlist_pane.screener_query = clause;
                    } else {
                        self.watchlist_pane.screener_query.push_str(" AND ");
                        self.watchlist_pane.screener_query.push_str(&clause);
                    }
                }
            }
        });
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.watchlist_pane.scan.is_none(),
                    egui::Button::new("Run Local NSE scan"),
                )
                .clicked()
            {
                let query = self.watchlist_pane.screener_query.clone();
                self.watchlist_pane.screener_notice = None;
                self.watchlist_pane.scan_report = None;
                let progress = Arc::new(ScanProgress::default());
                let worker_progress = progress.clone();
                self.watchlist_pane.scan_progress = Some(progress);
                self.watchlist_pane.scan = Some(
                    work_queue::SCREENER
                        .start(Some(ctx), move || scan_local_nse(query, worker_progress)),
                );
            }
            if let Some(progress) = &self.watchlist_pane.scan_progress {
                ui.spinner();
                ui.label(format!(
                    "Scanning {} / {} local symbols…",
                    progress.scanned.load(Ordering::Relaxed),
                    progress.total.load(Ordering::Relaxed)
                ));
                if ui.button("Cancel").clicked() {
                    progress.cancelled.store(true, Ordering::Relaxed);
                }
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Save as");
            ui.text_edit_singleline(&mut self.watchlist_pane.screener_name);
            if ui.button("Save query").clicked() {
                let name = self.watchlist_pane.screener_name.trim().to_owned();
                let text = self.watchlist_pane.screener_query.trim().to_owned();
                if name.is_empty() || text.is_empty() {
                    self.watchlist_pane.screener_notice =
                        Some("Enter both a query name and a condition.".to_owned());
                } else {
                    self.watchlist_pane
                        .saved_screens
                        .retain(|saved| saved.name != name);
                    self.watchlist_pane
                        .saved_screens
                        .push(SavedScreenerQuery { name, text });
                    self.persist_screener_queries();
                }
            }
        });
        let mut load = None;
        let mut remove = None;
        for (index, saved) in self.watchlist_pane.saved_screens.iter().enumerate() {
            ui.horizontal(|ui| {
                if ui.button(&saved.name).clicked() {
                    load = Some(index);
                }
                ui.small(&saved.text);
                if ui
                    .small_button("×")
                    .on_hover_text("Delete saved query")
                    .clicked()
                {
                    remove = Some(index);
                }
            });
        }
        if let Some(index) = load {
            let saved = self.watchlist_pane.saved_screens[index].clone();
            self.watchlist_pane.screener_name = saved.name;
            self.watchlist_pane.screener_query = saved.text;
        }
        if let Some(index) = remove {
            self.watchlist_pane.saved_screens.remove(index);
            self.persist_screener_queries();
        }
        if let Some(message) = &self.watchlist_pane.screener_notice {
            ui.colored_label(egui::Color32::LIGHT_RED, message);
        }
        if let Some(report) = &self.watchlist_pane.scan_report {
            if let Some(error) = &report.error {
                ui.colored_label(egui::Color32::LIGHT_RED, error);
                return;
            }
            ui.label(format!(
                "{} matches · {} scanned · {} unreadable{}",
                report.matches.len(),
                report.scanned,
                report.skipped,
                if report.cancelled {
                    " · cancelled"
                } else {
                    ""
                }
            ));
            let mut rows = report.matches.clone();
            let sort = self.watchlist_pane.screener_sort;
            rows.sort_by(|a, b| match sort {
                ScreenerSort::Symbol => a.symbol.cmp(&b.symbol),
                ScreenerSort::Close => a.close.total_cmp(&b.close),
                ScreenerSort::Return5 => a
                    .return_5
                    .unwrap_or(f64::NEG_INFINITY)
                    .total_cmp(&b.return_5.unwrap_or(f64::NEG_INFINITY)),
                ScreenerSort::Rsi14 => a
                    .rsi_14
                    .unwrap_or(f64::NEG_INFINITY)
                    .total_cmp(&b.rsi_14.unwrap_or(f64::NEG_INFINITY)),
                ScreenerSort::Volume => a.volume.total_cmp(&b.volume),
            });
            if self.watchlist_pane.screener_descending {
                rows.reverse();
            }
            ui.horizontal(|ui| {
                for (title, column) in [
                    ("Symbol", ScreenerSort::Symbol),
                    ("Close", ScreenerSort::Close),
                    ("5D %", ScreenerSort::Return5),
                    ("RSI 14", ScreenerSort::Rsi14),
                    ("Volume", ScreenerSort::Volume),
                ] {
                    if ui.small_button(title).clicked() {
                        if self.watchlist_pane.screener_sort == column {
                            self.watchlist_pane.screener_descending =
                                !self.watchlist_pane.screener_descending;
                        } else {
                            self.watchlist_pane.screener_sort = column;
                            self.watchlist_pane.screener_descending =
                                column != ScreenerSort::Symbol;
                        }
                    }
                }
            });
            egui::ScrollArea::vertical()
                .max_height(250.0)
                .show(ui, |ui| {
                    egui::Grid::new("local-nse-screen-results")
                        .striped(true)
                        .show(ui, |ui| {
                            for row in &rows {
                                ui.label(&row.symbol);
                                ui.label(format!("{:.2}", row.close));
                                ui.label(
                                    row.return_5
                                        .map_or("—".to_owned(), |value| format!("{value:.2}%")),
                                );
                                ui.label(
                                    row.rsi_14
                                        .map_or("—".to_owned(), |value| format!("{value:.1}")),
                                );
                                ui.label(format!("{:.0}", row.volume));
                                ui.end_row();
                            }
                        });
                });
            if ui.button("Save matches as Watchlist").clicked() {
                self.save_screen_matches(
                    report
                        .matches
                        .iter()
                        .map(|row| row.symbol.clone())
                        .collect(),
                );
            }
        }
    }

    fn save_screen_matches(&mut self, symbols: Vec<String>) {
        let name = if self.watchlist_pane.screener_name.trim().is_empty() {
            "Screener matches".to_owned()
        } else {
            self.watchlist_pane.screener_name.trim().to_owned()
        };
        let mut store = lock(&self.watchlist);
        match store.create_list(&name, None) {
            Ok(id) => {
                let changes = crate::store::watchlist::ListChanges {
                    add: symbols
                        .into_iter()
                        .map(|symbol| {
                            (
                                WatchlistInstrument::from_market(
                                    DataSource::Local,
                                    &symbol,
                                    &symbol,
                                ),
                                crate::store::watchlist::Place::End,
                            )
                        })
                        .collect(),
                    ..Default::default()
                };
                if let Err(error) = store.edit_list(id, changes) {
                    // The list was just created by this operation; do not leave an empty
                    // artifact when the bulk add is refused (for example, a full list).
                    let _ = store.delete_list(id);
                    self.watchlist_pane.screener_notice = Some(error.to_string());
                } else {
                    self.watchlist_pane.screener_notice =
                        Some(format!("Saved {name} as a normal watchlist."));
                    self.watchlist_pane.screener_visible = false;
                }
            }
            Err(error) => self.watchlist_pane.screener_notice = Some(error.to_string()),
        }
    }

    /// Closing the sidebar stops its quote demand at once; a background tab or a covered
    /// window never reaches here and is covered by the demand's lease.
    pub(super) fn suspend_watchlist_demand(&self) {
        self.quote_coordinator
            .set_sidebar_demand(self.id, Vec::new(), false);
    }

    fn apply_watchlist_actions(&mut self, actions: Vec<WatchlistAction>, ctx: &egui::Context) {
        let mut opens: Vec<(bool, WatchlistInstrument)> = Vec::new();
        {
            let mut store = lock(&self.watchlist);
            for action in actions {
                match action {
                    WatchlistAction::OpenScreener => self.watchlist_pane.screener_visible = true,
                    WatchlistAction::Open(item) => opens.push((true, item)),
                    WatchlistAction::AddChart(item) => opens.push((false, item)),
                    WatchlistAction::OpenInNewTab(item) => {
                        // Hand off after the store lock is released.
                        self.watchlist_pane.open_in_new_tab = Some(item);
                    }
                    WatchlistAction::AddSymbol { list } => {
                        if let Some(list_id) = list
                            && let Some(name) = store
                                .lists()
                                .iter()
                                .find(|candidate| candidate.id == list_id)
                                .map(|candidate| candidate.name.clone())
                        {
                            self.watchlist_pane.picker_target = Some(list_id);
                            self.symbol_picker.open_for_watchlist(&name);
                        }
                    }
                    WatchlistAction::CreateAlert(item) => {
                        self.alerts.request(AlertRequest::market(AlertAsset::new(
                            item.instrument.clone(),
                            crate::ui::watchlist::rows::icon_base_symbol(&item),
                            item.name.clone(),
                        )));
                    }
                    WatchlistAction::CreateList { name, with } => {
                        let result = store.create_list(&name, with).map(|_| ());
                        report_watchlist(&mut self.watchlist_pane.notice, result);
                    }
                    WatchlistAction::RenameList { id, name } => {
                        let result = store.rename_list(id, &name).map(|_| ());
                        report_watchlist(&mut self.watchlist_pane.notice, result);
                    }
                    WatchlistAction::DuplicateList { id, name } => {
                        let result = store.duplicate_list(id, &name).map(|_| ());
                        report_watchlist(&mut self.watchlist_pane.notice, result);
                    }
                    WatchlistAction::DeleteList(id) => {
                        let result = store.delete_list(id).map(|_| ());
                        report_watchlist(&mut self.watchlist_pane.notice, result);
                    }
                    WatchlistAction::AddToList { list, item } => {
                        let result = store.add_to_list(list, item).map(|_| ());
                        report_watchlist(&mut self.watchlist_pane.notice, result);
                    }
                    WatchlistAction::RemoveFromList { list, instrument } => {
                        let result = store.remove_from_list(list, &instrument).map(|_| ());
                        report_watchlist(&mut self.watchlist_pane.notice, result);
                    }
                    WatchlistAction::RemoveEntry { list, entry } => {
                        let result = store.remove_entry(list, entry).map(|_| ());
                        report_watchlist(&mut self.watchlist_pane.notice, result);
                    }
                    WatchlistAction::MoveToSection {
                        list,
                        entry,
                        section,
                    } => {
                        let result = store.move_instruments(list, &[entry], section).map(|_| ());
                        report_watchlist(&mut self.watchlist_pane.notice, result);
                    }
                    WatchlistAction::Move { list, mv } => {
                        let result = store.move_entries(list, &mv.ids, mv.before).map(|_| ());
                        report_watchlist(&mut self.watchlist_pane.notice, result);
                    }
                    WatchlistAction::MoveToSectionByDrop { list, mv } => {
                        let result = store
                            .move_instruments(list, &mv.ids, mv.section)
                            .map(|_| ());
                        report_watchlist(&mut self.watchlist_pane.notice, result);
                    }
                    WatchlistAction::AddSection { list, title } => {
                        let result = store.add_section(list, &title).map(|_| ());
                        report_watchlist(&mut self.watchlist_pane.notice, result);
                    }
                    WatchlistAction::RenameSection {
                        list,
                        section,
                        title,
                    } => {
                        let result = store.rename_section(list, section, &title).map(|_| ());
                        report_watchlist(&mut self.watchlist_pane.notice, result);
                    }
                    WatchlistAction::DeleteSection { list, section } => {
                        let result = store.delete_section(list, section).map(|_| ());
                        report_watchlist(&mut self.watchlist_pane.notice, result);
                    }
                    WatchlistAction::SetSectionCollapsed {
                        list,
                        section,
                        collapsed,
                    } => {
                        let result = store
                            .set_section_collapsed(list, section, collapsed)
                            .map(|_| ());
                        report_watchlist(&mut self.watchlist_pane.notice, result);
                    }
                    WatchlistAction::SetSort { list, sort } => {
                        let result = store.set_sort(list, sort).map(|_| ());
                        report_watchlist(&mut self.watchlist_pane.notice, result);
                    }
                    WatchlistAction::SetColumn {
                        list,
                        column,
                        visible,
                    } => {
                        let result = store.set_column(list, column, visible).map(|_| ());
                        report_watchlist(&mut self.watchlist_pane.notice, result);
                    }
                    WatchlistAction::SetShowsDescription { list, shows } => {
                        let result = store.set_shows_description(list, shows).map(|_| ());
                        report_watchlist(&mut self.watchlist_pane.notice, result);
                    }
                    WatchlistAction::SetShowsChart { list, shows } => {
                        let result = store.set_shows_chart(list, shows).map(|_| ());
                        report_watchlist(&mut self.watchlist_pane.notice, result);
                    }
                    WatchlistAction::SetFlag { instrument, flag } => {
                        store.set_flag(&instrument, flag);
                    }
                    WatchlistAction::Import { list, text } => {
                        match store.import_into(list, &text) {
                            Ok(report) => {
                                self.watchlist_pane.import_report = Some(report);
                                self.watchlist_pane.notice = None;
                            }
                            Err(error) => {
                                report_watchlist(&mut self.watchlist_pane.notice, Err(error))
                            }
                        }
                    }
                    WatchlistAction::BeginImport => self.watchlist_pane.import_report = None,
                    WatchlistAction::ChooseImportFile => {
                        self.watchlist_pane
                            .sidebar
                            .read_import_file(read_import_file(ctx));
                    }
                    WatchlistAction::Export { list } => {
                        if let Some(text) = store.export(list) {
                            let name = store
                                .lists()
                                .iter()
                                .find(|candidate| candidate.id == list)
                                .map(|candidate| candidate.name.clone())
                                .unwrap_or_else(|| "Watchlist".to_owned());
                            self.watchlist_pane.file_result =
                                Some(export_to_file(ctx, &name, text));
                        }
                    }
                    WatchlistAction::CopyText(text) => ctx.copy_text(text),
                    WatchlistAction::DismissError => {
                        self.watchlist_pane.notice = None;
                        store.clear_error();
                    }
                }
            }
        }
        for (open, item) in opens {
            if open {
                self.open_watchlist_instrument(&item, ctx);
            } else {
                self.add_chart_from_instrument(&item, ctx);
            }
        }
    }

    pub(super) fn open_watchlist_instrument(
        &mut self,
        item: &WatchlistInstrument,
        ctx: &egui::Context,
    ) {
        // A card is not a market: show the one already here, else add it beside the others
        // (never replace the focused chart's market with it).
        if let Some(card) = item.card {
            self.add_card(card);
            return;
        }
        let instrument = &item.instrument;
        if let Some(existing) = self.charts.iter().position(|chart| {
            InstrumentId::new(chart.source, &chart.symbol).is_same_market(instrument)
        }) {
            let chart_id = self.charts[existing].id;
            self.charts[existing].apply_watchlist_metadata(item);
            self.focus_chart(chart_id);
            // The focused market is the tab's leader: this is a change of it like any other.
            self.announce_market(item);
            return;
        }
        if let Some(focused) = self.resolved_focused_id() {
            self.replace_chart_market(focused, item, ctx);
            self.announce_market(item);
        } else {
            self.add_chart_from_instrument(item, ctx);
        }
    }

    pub(super) fn add_chart_from_instrument(
        &mut self,
        item: &WatchlistInstrument,
        ctx: &egui::Context,
    ) {
        if let Some(card) = item.card {
            self.add_card(card);
            return;
        }
        let outcome = self.add_chart_for(&item.instrument, ctx);
        if let Some(chart) = self
            .charts
            .iter_mut()
            .find(|chart| chart.id == outcome.chart_id)
        {
            chart.apply_watchlist_metadata(item);
        }
    }
}
