//! Pure, deterministic research-run domain values.
//!
//! The executor is deliberately independent of live paper quotes, storage and UI. A caller feeds
//! one completed daily bar at a time and receives an auditable lifecycle event.

use crate::model::Kline;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UniverseMember {
    pub source: String,
    pub symbol: String,
    /// Every selected watchlist/snapshot that contained this symbol.
    pub universe_labels: Vec<String>,
}

/// Immutable input to a forward-only paper campaign. The symbols and labels are copied when the
/// campaign is started; it never follows later edits to a Watchlist or screener result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FrozenUniverse {
    pub as_of_ms: i64,
    pub members: Vec<UniverseMember>,
}

impl FrozenUniverse {
    pub fn from_sets(
        as_of_ms: i64,
        sets: impl IntoIterator<Item = (String, Vec<(String, String)>)>,
    ) -> Self {
        Self {
            as_of_ms,
            members: merge_universes(sets),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CampaignStatus {
    Active,
    Stopped,
}

/// A forward-only campaign definition. Historical runners must never create this value or submit
/// a paper order; a host creates it explicitly after freezing the universe and strategy version.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaperCampaign {
    pub id: String,
    pub strategy_version: String,
    pub account_id: u64,
    pub universe: FrozenUniverse,
    pub status: CampaignStatus,
    #[serde(default)]
    pub submitted_order_ids: Vec<u64>,
    /// Keys for completed daily entry evaluations.  This prevents a user pressing
    /// "Evaluate" twice from creating a second order for the same symbol/bar.
    #[serde(default)]
    pub evaluated_signal_keys: Vec<String>,
    /// Risk settings copied from the immutable strategy at campaign creation.
    #[serde(default)]
    pub stop_loss_pct: f64,
    #[serde(default)]
    pub target_pct: f64,
}

/// Immutable strategy configuration captured by every run and campaign.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StrategyVersion {
    pub id: String,
    pub name: String,
    pub created_at_ms: i64,
    pub summary: String,
    pub context: StageDefinition,
    pub setup: StageDefinition,
    pub entry: StageDefinition,
    pub invalidation: String,
    pub exit_rule: String,
    pub stop_loss_pct: f64,
    pub target_pct: f64,
    pub max_holding_bars: usize,
    #[serde(default = "default_reb_setup_max_age")]
    pub reb_setup_max_age: usize,
    pub commission_pct: f64,
    pub slippage_pct: f64,
    pub execution: ExecutionModel,
    #[serde(default)]
    pub rule: StrategyRule,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StageDefinition {
    pub signal: String,
    pub trigger: String,
    pub confirmation: String,
}

impl UniverseMember {
    pub fn key(&self) -> String {
        format!("{}:{}", self.source, self.symbol)
    }
}

/// Deduplicates a Universe Set for execution while retaining all source labels for reports.
pub fn merge_universes(
    sets: impl IntoIterator<Item = (String, Vec<(String, String)>)>,
) -> Vec<UniverseMember> {
    let mut members: Vec<UniverseMember> = Vec::new();
    for (label, symbols) in sets {
        for (source, symbol) in symbols {
            if let Some(existing) = members
                .iter_mut()
                .find(|member| member.source == source && member.symbol == symbol)
            {
                if !existing.universe_labels.contains(&label) {
                    existing.universe_labels.push(label.clone());
                }
            } else {
                members.push(UniverseMember {
                    source,
                    symbol,
                    universe_labels: vec![label.clone()],
                });
            }
        }
    }
    members
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Stage {
    Context,
    Setup,
    Entry,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum StageEvent {
    Signal,
    Trigger,
    Confirmation,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CandidateStatus {
    Waiting,
    Candidate,
    Entered,
    Cancelled,
    Expired,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AuditEvent {
    pub bar_time: i64,
    pub stage: Stage,
    pub event: StageEvent,
    pub accepted: bool,
    pub reason: String,
}

#[derive(Clone, Debug, Default)]
pub struct Lifecycle {
    context: u8,
    setup: u8,
    entry: u8,
    pub status: CandidateStatus,
    pub audit: Vec<AuditEvent>,
}

impl Default for CandidateStatus {
    fn default() -> Self {
        Self::Waiting
    }
}

impl Lifecycle {
    /// Applies one observed event. A stage must progress Signal, Trigger, Confirmation in order;
    /// a later stage cannot progress before its preceding stage is confirmed.
    pub fn apply(
        &mut self,
        bar_time: i64,
        stage: Stage,
        event: StageEvent,
        accepted: bool,
        reason: impl Into<String>,
    ) -> bool {
        let reason = reason.into();
        let required = match event {
            StageEvent::Signal => 0,
            StageEvent::Trigger => 1,
            StageEvent::Confirmation => 2,
        };
        let previous_confirmed = match stage {
            Stage::Context => true,
            Stage::Setup => self.context == 3,
            Stage::Entry => self.setup == 3,
        };
        let current = match stage {
            Stage::Context => &mut self.context,
            Stage::Setup => &mut self.setup,
            Stage::Entry => &mut self.entry,
        };
        let valid = accepted && previous_confirmed && *current == required;
        self.audit.push(AuditEvent {
            bar_time,
            stage,
            event,
            accepted: valid,
            reason,
        });
        if valid {
            *current += 1;
        }
        if self.entry == 3 {
            self.status = CandidateStatus::Entered;
        } else if self.setup == 3 {
            self.status = CandidateStatus::Candidate;
        }
        valid
    }
    pub fn cancel(&mut self) {
        if self.status != CandidateStatus::Entered {
            self.status = CandidateStatus::Cancelled;
        }
    }
    pub fn expire(&mut self) {
        if self.status != CandidateStatus::Entered {
            self.status = CandidateStatus::Expired;
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ExecutionModel {
    SignalClose,
    NextBarOpen,
}

/// The executable rule attached to an immutable strategy definition. Text fields remain its
/// human-readable contract; this value makes the evaluated rule explicit.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum StrategyRule {
    #[default]
    PreviousHighClose,
    RebBreakout2,
}

const fn default_reb_setup_max_age() -> usize {
    20
}

/// Returns the fill price available to a historical test without reading any future candle.
pub fn historical_fill(
    model: ExecutionModel,
    signal_bar: &Kline,
    next_bar: Option<&Kline>,
) -> Option<f64> {
    match model {
        ExecutionModel::SignalClose => Some(signal_bar.close),
        ExecutionModel::NextBarOpen => next_bar.map(|bar| bar.open),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HistoricalConfig {
    pub execution: ExecutionModel,
    pub stop_loss_pct: f64,
    pub target_pct: f64,
    pub max_holding_bars: usize,
    pub commission_pct: f64,
    pub slippage_pct: f64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ExitReason {
    Target,
    Stop,
    Time,
    EndOfData,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HistoricalTrade {
    pub entry_bar: usize,
    pub exit_bar: usize,
    pub entry: f64,
    pub exit: f64,
    pub net_return_pct: f64,
    pub reason: ExitReason,
    #[serde(default)]
    pub same_bar_ambiguous: bool,
}

/// Deterministic long-only OHLC simulator. `entries[i]` is known only at the close of candle i.
/// It therefore fills at that close or the next bar's open; no future bar is read to create entry.
pub fn simulate_long(
    candles: &[Kline],
    entries: &[bool],
    config: HistoricalConfig,
) -> Vec<HistoricalTrade> {
    let mut trades = Vec::new();
    let mut open: Option<(usize, f64)> = None;
    for i in 0..candles.len() {
        let bar = &candles[i];
        if let Some((entry_bar, entry)) = open {
            let stop = entry * (1.0 - config.stop_loss_pct / 100.0);
            let target = entry * (1.0 + config.target_pct / 100.0);
            let same_bar_ambiguous = bar.low <= stop && bar.high >= target;
            let outcome = if bar.low <= stop {
                Some((stop, ExitReason::Stop))
            } else if bar.high >= target {
                Some((target, ExitReason::Target))
            } else if i.saturating_sub(entry_bar) >= config.max_holding_bars {
                Some((bar.close, ExitReason::Time))
            } else {
                None
            };
            if let Some((exit, reason)) = outcome {
                let gross = (exit - entry) / entry * 100.0;
                let net = gross - 2.0 * (config.commission_pct + config.slippage_pct);
                trades.push(HistoricalTrade {
                    entry_bar,
                    exit_bar: i,
                    entry,
                    exit,
                    net_return_pct: net,
                    reason,
                    same_bar_ambiguous,
                });
                open = None;
            }
        }
        if open.is_none() && entries.get(i).copied().unwrap_or(false) {
            let fill = match config.execution {
                ExecutionModel::SignalClose => Some(bar.close),
                ExecutionModel::NextBarOpen => candles.get(i + 1).map(|next| next.open),
            };
            if let Some(fill) = fill {
                open = Some((i, fill));
            }
        }
    }
    if let Some((entry_bar, entry)) = open {
        let i = candles.len().saturating_sub(1);
        let exit = candles[i].close;
        let gross = (exit - entry) / entry * 100.0;
        trades.push(HistoricalTrade {
            entry_bar,
            exit_bar: i,
            entry,
            exit,
            net_return_pct: gross - 2.0 * (config.commission_pct + config.slippage_pct),
            reason: ExitReason::EndOfData,
            same_bar_ambiguous: false,
        });
    }
    trades
}

/// The first built-in entry rule. It is deliberately explicit and uses only the current close
/// and the preceding completed bar, making it safe for historical and forward evaluation.
pub fn close_above_previous_high_entries(candles: &[Kline]) -> Vec<bool> {
    candles
        .iter()
        .enumerate()
        .map(|(index, candle)| index > 0 && candle.close > candles[index - 1].high)
        .collect()
}

/// Detects the existing green Range Expansion Bar (REB) study without future-bar access.
/// A REB is a green, no-gap candle whose range is at least the 90th percentile of its completed
/// 100-bar range-percent window and whose body midpoint sits within 3.8% of the candle midpoint.
/// The first full upside breakout (`low > REB high`) creates a setup; the second creates entry.
/// A close below the REB low invalidates it and the setup expires after `max_setup_age` bars.
pub fn reb_breakout2_entries(candles: &[Kline], max_setup_age: usize) -> Vec<bool> {
    let mut entries = vec![false; candles.len()];
    let mut anchor: Option<(usize, f64, f64, bool)> = None;
    for index in 0..candles.len() {
        let bar = &candles[index];
        if let Some((anchor_index, high, low, first_breakout)) = anchor {
            if index.saturating_sub(anchor_index) > max_setup_age || bar.close < low {
                anchor = None;
            } else if bar.low > high {
                if first_breakout {
                    entries[index] = true;
                    anchor = None;
                } else {
                    anchor = Some((anchor_index, high, low, true));
                }
            }
        }
        if anchor.is_none() && is_green_reb(candles, index) {
            anchor = Some((index, bar.high, bar.low, false));
        }
    }
    entries
}

fn is_green_reb(candles: &[Kline], index: usize) -> bool {
    const LOOKBACK: usize = 100;
    if index + 1 < LOOKBACK || index == 0 {
        return false;
    }
    let bar = &candles[index];
    let range = (bar.high - bar.low).max(0.001);
    let mut ranges: Vec<f64> = candles[index + 1 - LOOKBACK..=index]
        .iter()
        .map(|candle| (candle.high - candle.low).max(0.001) / candle.close.max(0.001) * 100.0)
        .collect();
    ranges.sort_by(f64::total_cmp);
    let p90 = ranges[(LOOKBACK - 1) * 9 / 10];
    let range_pct = range / bar.close.max(0.001) * 100.0;
    let body_mid = (bar.open + bar.close) / 2.0;
    let candle_mid = (bar.high + bar.low) / 2.0;
    let centered = ((body_mid - candle_mid).abs() / range * 100.0) <= 3.8;
    let previous = &candles[index - 1];
    range_pct >= p90
        && bar.open >= previous.low
        && bar.open <= previous.high
        && centered
        && bar.close > bar.open
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlapping_universes_execute_once_and_keep_labels() {
        let members = merge_universes([
            (
                String::from("NSE 50"),
                vec![(String::from("local"), String::from("IDEA"))],
            ),
            (
                String::from("F&O"),
                vec![(String::from("local"), String::from("IDEA"))],
            ),
        ]);
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].universe_labels, ["NSE 50", "F&O"]);
    }

    #[test]
    fn frozen_campaign_universe_keeps_its_original_membership() {
        let frozen = FrozenUniverse::from_sets(
            123,
            [("Breakouts".into(), vec![("Local".into(), "IDEA".into())])],
        );
        let campaign = PaperCampaign {
            id: "campaign-1".into(),
            strategy_version: "REB v1".into(),
            account_id: 9,
            universe: frozen.clone(),
            status: CampaignStatus::Active,
            submitted_order_ids: Vec::new(),
            evaluated_signal_keys: Vec::new(),
            stop_loss_pct: 5.0,
            target_pct: 13.0,
        };
        assert_eq!(campaign.universe.as_of_ms, 123);
        assert_eq!(campaign.universe.members[0].symbol, "IDEA");
        assert_eq!(campaign.universe.members[0].universe_labels, ["Breakouts"]);
    }

    #[test]
    fn reb_breakout_two_enters_only_on_second_full_breakout() {
        let mut candles: Vec<Kline> = (0..99)
            .map(|index| Kline {
                open_time: index,
                close_time: index,
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.0,
                volume: 1.0,
            })
            .collect();
        // Previous bar keeps the REB anchor's open inside its range.
        candles.push(Kline {
            open_time: 99,
            close_time: 99,
            open: 100.0,
            high: 110.0,
            low: 90.0,
            close: 100.0,
            volume: 1.0,
        });
        candles.push(Kline {
            open_time: 100,
            close_time: 100,
            open: 98.0,
            high: 115.0,
            low: 95.0,
            close: 112.0,
            volume: 1.0,
        });
        candles.push(Kline {
            open_time: 101,
            close_time: 101,
            open: 116.0,
            high: 119.0,
            low: 116.0,
            close: 118.0,
            volume: 1.0,
        });
        candles.push(Kline {
            open_time: 102,
            close_time: 102,
            open: 117.0,
            high: 120.0,
            low: 116.0,
            close: 119.0,
            volume: 1.0,
        });
        let entries = reb_breakout2_entries(&candles, 10);
        assert!(!entries[101]);
        assert!(entries[102]);
    }
    #[test]
    fn stages_cannot_skip_confirmation_order() {
        let mut flow = Lifecycle::default();
        assert!(!flow.apply(1, Stage::Setup, StageEvent::Signal, true, "too early"));
        for event in [
            StageEvent::Signal,
            StageEvent::Trigger,
            StageEvent::Confirmation,
        ] {
            assert!(flow.apply(1, Stage::Context, event, true, "ok"));
        }
        for event in [
            StageEvent::Signal,
            StageEvent::Trigger,
            StageEvent::Confirmation,
        ] {
            assert!(flow.apply(2, Stage::Setup, event, true, "ok"));
        }
        for event in [
            StageEvent::Signal,
            StageEvent::Trigger,
            StageEvent::Confirmation,
        ] {
            assert!(flow.apply(3, Stage::Entry, event, true, "ok"));
        }
        assert_eq!(flow.status, CandidateStatus::Entered);
    }

    fn candle(open: f64, high: f64, low: f64, close: f64) -> Kline {
        Kline {
            open_time: 0,
            close_time: 0,
            open,
            high,
            low,
            close,
            volume: 1.0,
        }
    }
    #[test]
    fn simulator_waits_for_next_open_and_charges_round_trip_costs() {
        let candles = [
            candle(10.0, 10.0, 10.0, 10.0),
            candle(11.0, 12.0, 10.5, 12.0),
        ];
        let trades = simulate_long(
            &candles,
            &[true, false],
            HistoricalConfig {
                execution: ExecutionModel::NextBarOpen,
                stop_loss_pct: 5.0,
                target_pct: 10.0,
                max_holding_bars: 5,
                commission_pct: 0.1,
                slippage_pct: 0.1,
            },
        );
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].entry, 11.0);
        assert_eq!(trades[0].reason, ExitReason::EndOfData);
        assert!((trades[0].net_return_pct - 8.6909).abs() < 0.01);
    }
}
