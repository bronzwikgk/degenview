# Research Runner implementation plan by file

**Status:** Planned. No files in this plan have been created for the Research Runner yet.  
**Prerequisite:** Sign-off on `docs/research-runner-spec.md`.

## Implementation order

1. Add pure research domain types and tests.
2. Add local persistence for immutable definitions and results.
3. Add the historical execution host and background-run path.
4. Add the persisted top-level Research tab and its UI.
5. Add forward Paper Campaign orchestration using the existing Paper engine.
6. Add reports, diagnostics, tests and manual acceptance evidence.

Do not begin with UI-only placeholders. Each UI section must render persisted data or a deliberate
empty/loading/error state from its host.

## New files

| File | Responsibility | Phase |
| --- | --- | --- |
| `src/model/research/mod.rs` | Public pure domain module: identifiers, versioned strategy, universe snapshot, run configuration, lifecycle event and run status. | 1 |
| `src/model/research/strategy.rs` | Context/Setup/Entry Signal → Trigger → Confirmation conditions, invalidation, stop, exit and execution settings. No I/O. | 1 |
| `src/model/research/engine.rs` | Deterministic daily bar-by-bar historical state machine and fill model. Enforces as-of-bar access and same-bar stop/target policy. | 1 |
| `src/model/research/report.rs` | Pure trade ledger, symbol diagnostics, aggregate metrics, equity curve and report calculations. | 1 |
| `src/model/research/tests.rs` | Unit tests for stage ordering, pivot confirmation delay, candidate expiry, invalidation, next-open fills, costs and same-bar ambiguity. | 1 |
| `src/store/research.rs` | SQLite Store methods for strategies, universe snapshots, run configurations, reports, campaigns and audit events. | 2 |
| `src/host/research/mod.rs` | Stateful coordinator for saved records, background job state and UI-facing actions. | 3 |
| `src/host/research/historical.rs` | Starts/polls/cancels the historical run on a work-queue lane; converts OHLCV into pure engine input. | 3 |
| `src/host/research/campaign.rs` | Forward-only campaign coordinator: frozen universe, strategy version, daily evaluation and hand-off to the existing Paper host. | 5 |
| `src/ui/research/tab/mod.rs` | `ResearchTab` top-level view, internal section selection and common header. | 4 |
| `src/ui/research/universe.rs` | Watchlist/screener/snapshot selection, query loading, query result preview and snapshot action. | 4 |
| `src/ui/research/strategy_builder.rs` | Versioned builder for Context, Setup and Entry staged conditions plus invalidation/stop/exit/costs. | 4 |
| `src/ui/research/historical_test.rs` | Historical-test configuration, run progress, cancellation, loading/empty/error states. | 4 |
| `src/ui/research/reports.rs` | Summary, equity curve, trade ledger, signals, symbol analysis, parameters and diagnostics views. | 6 |
| `src/ui/research/paper_campaigns.rs` | Create, monitor and stop forward paper campaigns. | 5 |

## Existing files to modify

| File | Change | Phase |
| --- | --- | --- |
| `src/model/mod.rs` | Export `model::research`. | 1 |
| `src/store/mod.rs` | Register Store methods from `store::research`; classify new persisted records/local settings according to storage policy. | 2 |
| `src/service/work_queue.rs` | Add a dedicated research worker lane so long historical scans do not block chart, screener or snapshot work. | 3 |
| `src/app/tabs.rs` | Add `TabContent::Research`, title/icon dispatch, UI dispatch and top-level tab ownership. | 4 |
| `src/app/session.rs` | Add persisted `Research` special-tab kind and backwards-compatible session restoration. | 4 |
| `src/app/mod.rs` | Restore a persisted Research tab and wire its shared hosts. | 4 |
| `src/app/commands.rs` | Add `OpenResearch` application command. | 4 |
| `src/ui/workspace/empty_state/tabs.rs` | Change the existing New Tab Screener route to open the Research tab directly on the Universe section. | 4 |
| `src/ui/workspace/empty_state/mod.rs` | Replace the New Tab Screener action with `OpenResearch { section: Universe }`; update tests for the route. | 4 |
| `src/workspace/empty_page/mod.rs` | Dispatch the New Tab Screener action to the application-level Research-tab command rather than a Watchlist panel. | 4 |
| `src/workspace/watchlist.rs` | Retain local screener scanning/query persistence as a reusable service boundary; move only reusable query execution behind a narrow host/service API if needed. | 2–4 |
| `src/model/screener/mod.rs` | Reuse the parser/evaluator; extend only if the agreed universe grammar needs a pure additional field. | 2 |
| `src/host/paper/mod.rs` | Expose a narrow, auditable hand-off for approved forward campaign orders; do not let historical runs use live paper quotes. | 5 |
| `src/model/paper/engine.rs` | Reuse unchanged where possible for forward paper execution; historical fills stay in `model::research::engine`. | 5 |
| `src/mcp/` | Deferred. Add read-only Research Runner tools only after the tab, persistence and audit model are stable. | Post-v1 |

## Data boundaries

```text
ui::research
  → host::research
    → store::research                  (saved definitions/results)
    → service::work_queue              (background execution)
    → model::research::engine          (pure historical calculation)
    → host::paper                      (forward campaigns only)
```

`model::research` must not import Store, services, UI, workspace, host or live quote types.
`ui::research` must not query SQLite, parse data files or spawn threads directly.

## Existing components reused without changing their meaning

| Component | Research Runner use |
| --- | --- |
| `model::screener` | Parse/evaluate Local NSE universe conditions. |
| Watchlist storage | Select normal Watchlists as a universe source. |
| Local NSE OHLCV service | Historical daily-bar input. |
| `service::work_queue::Pending` | Background historical job lifecycle. |
| `host::paper` and `model::paper` | Forward campaign orders, brackets, commissions, slippage, positions and ledger. |
| Chart Replay | Manual visual validation only; never historical order fills. |
| Portfolio tab/session pattern | Model for a persisted special top-level tab. |

## Required test coverage by phase

| Phase | Required evidence |
| --- | --- |
| 1 | Pure unit tests prove no stage can skip Signal/Trigger/Confirmation and no pivot is available before confirmation. |
| 2 | Store round-trip tests preserve strategy versions, universe snapshots, runs and diagnostics. |
| 3 | Deterministic fixture tests verify expiry, invalidation, entry model, costs and no look-ahead. |
| 4 | UI headless tests cover empty, loading, running, completed and error states; manual screenshot/interaction review. |
| 5 | Tests prove historical jobs cannot call live paper pricing and paper campaigns retain their frozen universe/strategy version. |
| 6 | A reproducible sample run verifies ledger totals, net P&L, equity curve and diagnostic counts. |

## Not part of this file plan

- Live broker execution.
- Intraday execution.
- Multi-symbol Pine execution.
- Parameter optimisation.
- Automatic predictive recommendations.
