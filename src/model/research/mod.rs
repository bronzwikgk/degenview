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
                    existing.universe_labels.push(label);
                }
            } else {
                members.push(UniverseMember {
                    source,
                    symbol,
                    universe_labels: vec![label],
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
}
