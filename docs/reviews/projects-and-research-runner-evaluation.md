# Evaluation report: Projects tracker and Research Runner design

**Branch:** `gk/v3-projects`  
**Head commit:** `f7c36692`  
**Comparison base:** `main`  
**Prepared:** 2026-11-27

## Review request

Evaluate the implemented local Projects tracker and the completed design documentation for the
future Research Runner. The requested product direction is a DegenView workspace for:

1. local project/task tracking;
2. Local NSE screening and Watchlist conversion; and
3. a future separate Research Runner for multi-symbol historical testing and forward paper
   campaigns.

## Delivered implementation

### Projects tracker

The branch adds `src/workspace/projects.rs`, a local-first tracker stored under the versioned
setting key `gk.projects.v1`.

- Project fields: name, description, branch, status, dates and tasks.
- Task fields: title, description, status, priority, branch, dates and completion state.
- Project statuses: Idea, Planned, In Progress, Testing, Done and Blocked.
- Task priorities: Low, Normal, High and Critical.
- Seeded projects:
  - **DegenView V1**
  - **Algo Trading Price Action**
- Seeded task in both projects: define project single-word concepts and keywords.
- Local persistence uses the application Store rather than an external service.

### New Tab discovery route

The empty New Tab page now lists:

```text
Recent | Screener | Projects | Layouts
```

Favorites and Suggestions were removed from that launcher. The separate Screener and Projects
toolbar buttons beside Watchlist were removed.

### Screener integration retained

The existing Local NSE screener remains the source of screener functionality:

- Saved Local NSE condition queries.
- Query builder and daily-data scan.
- Conversion of matches into an ordinary Watchlist.
- Turnover template query.

## Research Runner design delivered, not implemented

The documents define the planned separate Research tab, but no historical multi-symbol execution
engine has been added yet.

Defined scope includes:

- Watchlist, saved screener query, new query and frozen-universe sources.
- Versioned Strategy definitions.
- Context, Setup and Entry stages, each with:

```text
Signal → Trigger → Confirmation
```

- Historical bar-by-bar execution with strict as-of-date access.
- Separate pivot occurrence and pivot confirmation dates.
- Close and next-bar-open execution models.
- Candidate lifecycle, invalidation, expiry, stop, target and time exits.
- Costs, slippage, diagnostics, equity curve and trade ledger requirements.
- Forward paper campaigns using a frozen universe and the existing Paper engine.

## Files for review

| Area | Files |
| --- | --- |
| Projects tracker | `src/workspace/projects.rs`, `src/workspace/mod.rs` |
| Workspace interaction | `src/workspace/toolbar.rs`, `src/workspace/empty_page/mod.rs`, `src/workspace/watchlist.rs` |
| New Tab UI | `src/ui/workspace/empty_state/mod.rs`, `src/ui/workspace/empty_state/tabs.rs`, `src/ui/workspace/toolbar.rs` |
| Research specification | `docs/research-runner-spec.md` |
| Research implementation checklist | `docs/research-runner-todo.md` |
| User-facing tracker change | `changes/projects-tracker.md` |
| AI work log | `CHANGELOG_OPENAI.md`, entries 10–11 |

## Commit trail

| Commit | Purpose |
| --- | --- |
| `8126be48` | Added local Projects tracker and workspace integration. |
| `02eccb2c` | Ignored local development build artifacts. |
| `5200b6a6` | Moved Screener/Projects discovery to New Tab launcher. |
| `5d30381f` | Added Research Runner specification and to-do. |
| `c7b07578` | Added Signal → Trigger → Confirmation lifecycle rules. |
| `f7c36692` | Updated change logs and corrected Projects route wording. |

## Verification evidence

- `cargo fmt --all` completed before build.
- `cargo build` completed successfully on 2026-11-27.
- Development executable produced:

```text
dist/dev/DegenView-GK-v0.6.0-rc.32-projects-dev.exe
89,157,632 bytes
```

- The build completed with 21 warnings, none fatal. Several warnings predate this work. Warnings
  introduced by the launcher removal are dead-code warnings for the old Favorites/Suggestions
  helpers and unused toolbar action variants; they should be cleaned before a production merge.

## Known limitations and review decisions required

1. **New Tab is a launch route, not yet a central full-tab screener/project renderer.** Selecting
   Screener or Projects on New Tab presents an open action that activates the existing associated
   workspace panel. This satisfies discovery relocation, but not the stricter interpretation of a
   fully central, tab-contained Screener/Projects workspace. Review whether the next increment
   should extract both renderers into central tab content.

2. **Projects tracker is local only.** It has no sync, sharing, import/export or MCP CRUD tools.

3. **Research Runner is specification-only.** It does not run historical tests, create paper
   campaigns, or generate reports yet.

4. **Existing chart Replay is not a backtest engine.** It hides future candles, but current paper
   trading uses current/live quotes; it must not be used to derive historical P&L.

5. **No manual visual acceptance run is recorded.** The executable exists, but an evaluator should
   launch it and verify UI interaction and persistence.

## Suggested acceptance checklist

- [ ] New Tab shows exactly Recent, Screener, Projects and Layouts in that order.
- [ ] Favorites and Suggestions do not appear in the New Tab launcher.
- [ ] Screener and Projects toolbar buttons do not appear beside Watchlist.
- [ ] Opening Projects shows the two seeded projects and their seeded tasks.
- [ ] Add, edit, complete and delete a task; restart the app; verify persistence.
- [ ] Open Screener, run/load a saved query and save matches as a Watchlist.
- [ ] Confirm the tracker and screener launch behaviour is acceptable, or request central full-tab
  renderers as the next implementation task.
- [ ] Confirm the Research Runner specification covers the required testing workflow before code
  implementation begins.

## Evaluation outcome requested

Please classify each area as **accepted**, **needs correction**, or **out of scope**:

1. Projects tracker data model and persistence.
2. New Tab navigation and placement.
3. Local NSE screener integration.
4. Research Runner specification.
5. Readiness to implement the Research Runner execution engine.
