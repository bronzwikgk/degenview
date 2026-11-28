# Research Runner to-do

Implementation map: `docs/research-runner-implementation-plan.md`.

Status: `[x]` verified implemented, `[-]` partially implemented, `[ ]` not implemented.

## Defined scope

- [x] Add persisted `Research` top-level tab alongside Chart and Portfolio.
- [x] Route New Tab **Screener** to Research Runner's Universe section.
- [x] Keep **Projects** as the development tracker route.
- [x] Reuse Local NSE screener query builder and saved-query storage in the Universe section.
- [x] Add Watchlist, saved query, new query and immutable universe snapshot sources.
- [x] Add Universe Sets: select multiple Watchlists and/or screener snapshots in one run.
- [x] Deduplicate identical source-qualified symbols for execution while retaining every universe-membership label in reports.
- [x] Save historical universe membership with its as-of date.
- [-] Add versioned strategy definitions with Context, Setup, Entry, Invalidation, Stop, Exit and Costs blocks. *(Immutable definitions are saved; rule execution remains limited to the built-in daily entry rule.)*
- [x] Give Context, Setup and Entry separate Signal, Trigger and Confirmation conditions.
- [-] Persist REB context/setup/entry confirmation and invalidation/expiry timestamps with reasons. *(Trigger, values, and fill linkage remain pending.)*
- [x] Enforce stage order: no trigger without its signal, no confirmation without its trigger, and no entry fill before entry confirmation.
- [x] Implement the REB / Breakout #2 strategy definition with a maximum setup age and opposite-side invalidation.
- [x] Build daily historical runner with strict as-of-bar data access and frozen universe membership.
- [ ] Record pivot occurrence date separately from pivot confirmation date.
- [x] Implement close and next-bar-open execution models.
- [x] Define conservative same-bar stop/target precedence (stop first) and report ambiguous cases.
- [-] Add candidate lifecycle: Candidate, Cancelled, Expired, Entered, Target, Stopped, Timed Out and Open. *(REB confirmation, invalidation, and expiry are recorded; complete per-candidate state/entry linkage remains pending.)*
- [-] Store auditable historical run results and diagnostics; do not suppress data/rule errors. *(Persisted runs, symbol diagnostics and REB lifecycle events are implemented; values/fill audit remains pending.)*
- [-] Add report views: Summary, Equity Curve, Trades, Signals, Symbols, Parameters and Diagnostics. *(Summary, trades, per-universe totals, win/loss, and drawdown are implemented; dedicated remaining views pending.)*
- [x] Show separate reports per source universe and a clearly labelled combined report for a Universe Set.
- [-] Add forward-only Paper Campaigns from frozen Watchlist/screener snapshots. *(Frozen campaigns can be manually evaluated; automatic daily scheduling remains pending.)*
- [-] Reuse the existing paper engine for forward orders, brackets, commissions, slippage and ledger records. *(Manual campaign evaluation submits deduplicated auditable Paper Host market orders with saved stop/target brackets; scheduling and campaign-level cost configuration remain pending.)*
- [x] Verify the historical runner never sends replay or live quotes to the historical fill model.
- [-] Test the same strategy on two overlapping universe snapshots and verify independent ledgers, labels and combined totals. *(Deduplication/labels are tested; independent per-universe ledgers remain pending.)*

## Existing components to reuse

- Local NSE screener and saved queries.
- Watchlists and screener-match-to-watchlist conversion.
- Chart Replay for manual visual validation only.
- Paper engine: orders, positions, brackets, commission, slippage, fills and closed-trade export.
- Existing top-level Portfolio tab persistence pattern.

## Explicit exclusions

- Live broker execution.
- Intraday testing.
- Multi-symbol Pine strategy execution.
- Parameter optimization.

## Review gates learned from evaluation

- [ ] Obtain sign-off on `research-runner-spec.md` before implementing the execution engine.
- [ ] Label all reports and release notes as Specified, Implemented, or Visually accepted; do not
  imply that a documented feature is executable.
- [ ] Add an explicit manual acceptance checklist to each built increment and record its result.
- [ ] Decide whether Screener and Projects must become central full-tab renderers instead of New
  Tab launch routes to their existing workspace panels.
- [ ] Keep reviewer-facing Markdown in UTF-8 so symbols and punctuation render consistently.
