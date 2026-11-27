# Research Runner to-do

## Defined scope

- [ ] Add persisted `Research` top-level tab alongside Chart and Portfolio.
- [ ] Route New Tab **Screener** to Research Runner's Universe section.
- [ ] Keep **Projects** as the development tracker route.
- [ ] Reuse Local NSE screener query builder and saved-query storage in the Universe section.
- [ ] Add Watchlist, saved query, new query and immutable universe snapshot sources.
- [ ] Save historical universe membership with its as-of date.
- [ ] Add versioned strategy definitions with Context, Setup, Entry, Invalidation, Stop, Exit and Costs blocks.
- [ ] Give Context, Setup and Entry separate Signal, Trigger and Confirmation conditions.
- [ ] Persist the signal, trigger, confirmation and execution timestamps, values and failure reasons for every trade lifecycle.
- [ ] Enforce stage order: no trigger without its signal, no confirmation without its trigger, and no entry fill before entry confirmation.
- [ ] Implement the REB / Breakout #2 strategy definition with a maximum setup age and opposite-side invalidation.
- [ ] Build daily historical runner with strict as-of-bar data access.
- [ ] Record pivot occurrence date separately from pivot confirmation date.
- [ ] Implement close and next-bar-open execution models.
- [ ] Define same-bar stop/target precedence and report ambiguous cases.
- [ ] Add candidate lifecycle: Candidate, Cancelled, Expired, Entered, Target, Stopped, Timed Out and Open.
- [ ] Store auditable historical run results and diagnostics; do not suppress data/rule errors.
- [ ] Add report views: Summary, Equity Curve, Trades, Signals, Symbols, Parameters and Diagnostics.
- [ ] Add forward-only Paper Campaigns from frozen Watchlist/screener snapshots.
- [ ] Reuse the existing paper engine for forward orders, brackets, commissions, slippage and ledger records.
- [ ] Verify the historical runner never sends replay or live quotes to the historical fill model.

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
