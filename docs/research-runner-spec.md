# Research Runner specification

## Purpose

Provide a dedicated DegenView tab for repeatable historical testing and forward paper campaigns
over a selected Watchlist or Local NSE screener result. It must prevent future-data leakage and
make every result reproducible.

This is not a Pine strategy runner. Pine remains chart-local. Research Runner evaluates a saved
rule definition across a multi-symbol universe.

## Entry point and tab

Add a persisted top-level application tab, alongside Chart and Portfolio:

```rust
TabContent::Research(Box<ResearchTab>)
```

The New Tab launcher order is:

```text
Recent | Screener | Projects | Layouts
```

The Screener entry opens Research Runner directly to **Universe**. The Projects entry remains the
local development tracker. Research Runner itself contains these sections:

```text
Universe | Strategy | Historical Test | Paper Campaigns | Reports
```

## Universe

### Sources

- Existing Watchlist.
- Saved Local NSE screener query.
- New Local NSE screener query.
- A previously saved universe snapshot.

### Required behaviour

- Reuse the existing Local NSE screener grammar and query builder.
- Allow a user to run, name, save, load and delete screener queries.
- Allow screener matches to be saved as a normal Watchlist.
- Allow a test or campaign to store an immutable universe snapshot: symbols, data source,
  timeframe, creation time and, for historical work, the as-of date.
- A historical run must evaluate membership using only data known at its as-of date. It must not
  use today's screener membership for an earlier test date.

## Strategy definition

A strategy is a versioned, saved definition made of these required blocks:

1. **Trade Context** — conditions that permit setup detection.
2. **Trade Setup** — conditions that create a candidate.
3. **Entry** — trigger plus execution timing.
4. **Invalidation** — conditions that cancel a candidate before entry.
5. **Stop Loss** — initial protective stop rule.
6. **Exit** — target, stop, time exit, or defined structural exit.
7. **Execution and costs** — quantity/position size, fill timing, brokerage and slippage.

Each saved strategy version must retain its plain-language summary, machine-readable parameters,
created time and immutable version ID. A completed run keeps the exact strategy version it used.

### Signal, trigger and confirmation model

Every evaluable strategy block uses the same ordered three-stage contract:

| Stage | Meaning | Historical test rule |
| --- | --- | --- |
| **Signal** | A condition is visible and makes the next stage eligible. | Record its bar and values; it does not itself advance a trade. |
| **Trigger** | The event that acts on the active signal. | Record the trigger bar; it starts the confirmation window/state. |
| **Confirmation** | The required validation of that trigger. | Advance only when confirmation is available using bars known at that time. |

This applies independently to **Trade Context**, **Trade Setup**, and **Entry**:

```text
Context signal → context trigger → context confirmation
    → Setup signal → setup trigger → setup confirmation
        → Entry signal → entry trigger → entry confirmation → execute trade
```

Each stage stores its occurrence time, confirmation time where different, condition result,
relevant prices/values, and failure reason. A later stage cannot occur without its own prior stage
being active. If a confirmation needs future bars, the strategy must name its confirmation delay;
the runner exposes the result only when those bars have closed.

For Entry, the confirmation is distinct from execution: the configured close or next-bar-open fill
happens only after entry confirmation. The report records signal, trigger, confirmation and fill as
separate events.

### Initial REB strategy requirements

The first supported strategy must express the existing REB / Breakout #2 study as explicit rules:

```text
Context signal:    REB detected.
Context trigger:   REB range is registered as active.
Context confirm:   REB remains valid under the configured context confirmation rule.
Setup signal:      First full-range breakout above REB high.
Setup trigger:     The first breakout closes and starts the setup state.
Setup confirm:     The configured setup confirmation rule validates the first breakout.
Entry signal:      A confirmed setup remains active.
Entry trigger:     Second full-range breakout above REB high.
Entry confirm:     The configured entry confirmation rule validates the second breakout.
Invalidation:      Close below REB low before entry.
Setup expiry:      User-defined maximum bars after REB.
Stop:              REB low.
Exit:              User-defined target, confirmed HH, stop, or time exit.
Execution:         User-selected close or next-bar-open model.
```

No trade condition may silently use future bars. A pivot that needs N right-side bars becomes
available only on its confirmation bar, N bars after its pivot bar.

## Historical Test

### Inputs

- Universe or immutable universe snapshot.
- Strategy version.
- Timeframe, initially daily only.
- Start and end date.
- Initial capital.
- Position-sizing rule.
- Maximum concurrent positions.
- Commission/brokerage and slippage configuration.
- Explicit execution model: signal-close or next-bar-open.

### Event processing rule

For every symbol and each bar at date `T`:

```text
1. Expose only OHLCV and indicator/pivot confirmations available at T.
2. Evaluate context, setup, candidate expiry and invalidation.
3. Submit eligible entries using the selected execution model.
4. Evaluate protective stop and exit rules without assuming intrabar order.
5. Update trades, capital, positions and audit events.
```

The simulator must define and report its same-bar ordering rule when target and stop are both
touched. It must not fill at a price not reachable from the chosen OHLC model.

### Required states

```text
Candidate | Cancelled | Expired | Entered | Target | Stopped | Timed Out | Open
```

`Open` means the configured test end was reached while the trade remained open. It must not be
mixed with a resolved loss or win.

## Paper Campaigns

Paper Campaigns use the same saved strategy version but process new data only after activation.

- Choose and freeze a Watchlist or screener-result snapshot at campaign creation.
- Save activation timestamp, initial capital, costs and strategy version.
- Add signals/candidates as new eligible daily data arrives.
- Reuse the existing Paper engine for order, position, stop/target, commission, slippage and
  closed-trade persistence.
- Display that campaign trades are forward paper trades, not historical fills.

The current chart replay must not be treated as historical paper execution: it intentionally hides
future candles, but existing paper orders are priced from current/live quotes.

## Reports

Each completed historical run and paper campaign must retain an audit record and show:

### Run summary

- Universe size and symbols actually processed.
- Strategy version and parameter values.
- Date range, timeframe and execution model.
- Context, setup and entry signal/trigger/confirmation counts.
- Candidate, cancelled, expired, entered and open counts.
- Trades, net return, win rate, average win/loss, expectancy, maximum drawdown and exposure.

### Trade ledger

- Symbol and data source.
- Context, setup and entry signal/trigger/confirmation dates and reasons.
- Signal date, pivot occurrence date where relevant, pivot confirmation date, entry date and exit
  date.
- Entry, stop, target and exit prices.
- Entry/exit reason, gross P&L, brokerage, slippage, net P&L.
- Holding bars/days, maximum favourable excursion and maximum adverse excursion.

### Diagnostics

- Symbols skipped and exact reason: missing data, malformed data, insufficient history, or rule
  evaluation failure.
- Same-bar stop/target ambiguity count.
- No silent exception suppression.

### Views

```text
Summary | Equity Curve | Trades | Signals | Symbols | Parameters | Diagnostics
```

## Persistence

Persist locally using explicit versioned records for:

- Saved screener queries.
- Universe snapshots.
- Strategy definitions and versions.
- Historical run configuration and results.
- Paper campaign configuration, state and audit events.

Persist enough information to reproduce a report without changing it when a current screener query
or strategy is edited later.

## Out of scope for the first release

- Live broker order placement.
- Intraday testing.
- Pine execution across a symbol universe.
- Parameter optimization.
- Predictive claims or signal recommendations.

## Review and acceptance gates

Every increment must identify its delivery state precisely:

| State | Meaning |
| --- | --- |
| **Specified** | Requirements are documented; no executable behaviour is claimed. |
| **Implemented** | Source code exists and has passed its relevant automated checks. |
| **Visually accepted** | A user has exercised the built application against an explicit acceptance checklist. |

The Research Runner execution engine may not be described as ready merely because this
specification is complete. Before code implementation begins, the specification must be signed off.
Before an implemented increment is accepted, its development executable must be checked manually
against its acceptance checklist.

The UI decision for Screener and Projects remains explicit: the current New Tab entries launch
their existing workspace panels. A later increment must decide whether those panels remain launch
targets or are extracted into central, full-tab renderers.

