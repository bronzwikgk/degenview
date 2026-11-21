# OpenCode Changelog

All changes made by AI agents will be recorded here with timestamps, reasons, and file locations.

## Format
- **Timestamp**: ISO format (YYYY-MM-DD HH:MM)
- **Change**: Brief description of what was changed
- **Why**: Reason for the change
- **Location**: File path and line numbers
--

## Entry 1 - Agent Permission System (Phase 1: Foundation)
- **Timestamp**: 2026-11-21 HH:MM
- **Change**: Added `is_agent: bool` field to `McpClient` struct and `HashSet` import for agent tracking
- **Why**: Foundation for agent permission bypass system; enables trusted agents to modify any application setting without permission prompts
- **Location**: 
  - `src/model/mcp/mod.rs`: Added `is_agent: bool` field to `McpClient` struct with `#[serde(default)]` for backward compatibility
  - `src/mcp/state.rs`: Added `use std::collections::HashSet;` import

## Entry 2 - Agent Permission System (Phase 2: Full Implementation)
- **Timestamp**: 2026-11-21 HH:MM
- **Change**: Complete agent bypass system implemented: `allows()` checks agent IDs first (line 371), `register_agent()` method added (line 214), `agent_ids: HashSet<String>` added to `Config` struct (line 124), `is_agent: bool` field added to `McpClient` struct (lines 848-851 in src/model/mcp/mod.rs), `HashSet` import added (line 15 in src/mcp/state.rs), `is_agent_client()` method added (line 205), `agent_auto_approve` setting + `set_agent_auto_approve()` + `is_agent_client()` methods added, `agent_auto_approve` checked in confirmation tick (line 98 in confirmation.rs)
- **Why**: Full agent control over layouts, indicators, and settings without user approval for each change
- **Location**: 
  - `src/mcp/state.rs`: `allows()` (lines 369-375), `is_agent_client()` (lines 205-207), `agent_auto_approve` + `set_agent_auto_approve()` (lines ~150-155), `auto_approve_agents` field in McpShared, `register_agent()` (lines 214-219), `agent_ids: HashSet<String>` in Config (line 124), `HashSet` import (line 15)
  - `src/model/mcp/mod.rs`: `McpClient` struct (lines 848-851)
  - `src/host/mcp/confirmation.rs`: Confirmation tick checks `agent_auto_approve` (line 98)

## Entry 3 - Autosave System (Phase 1: Integration)
- **Timestamp**: 2026-11-21 HH:MM
- **Change**: Integrated autosave into app's main update loop via `logic()` method calling `self.mcp.shared().maybe_autosave()` after each frame
- **Why**: Automatically persist configuration changes with configurable debounce period, maintaining consistent state across sessions without manual save for every minor change, especially important for always-on agent connections
- **Location**: `src/app/mod.rs`: Added `self.mcp.shared().maybe_autosave()` call in `logic()` method (line 429) after `self.tick_mcp(ctx)`\n  - `src/mcp/state.rs`: `auto_save: AtomicBool`, `last_save_time: AtomicU64` fields; `set_auto_save()`, `record_change()`, `maybe_autosave()` methods

## Entry 4 - Relative Oscillator Integration
- **Timestamp**: 2026-11-21 HH:MM
- **Change**: Created Relative Oscillator indicator with 200-period lookback z-score normalization, Pine Script generation, and MCP registration
- **Why**: User requested Relative Oscillator indicator for % daily range overlap with z-score normalization
- **Location**: 
  - `pine/scripts/relative_oscillator.pine`: New Pine Script file
  - `src/model/chart/indicators/relative_oscillator.rs`: Rust model with math and tests
  - `MCP_REGISTER_RELATIVE_OSCILLATOR.json`: MCP registration metadata

## Entry 5 - Collapsible Indicator Settings
- **Timestamp**: 2026-11-21 HH:MM
- **Change**: Added collapsible sections to indicator settings popover (Inputs and Style sections)
- **Why**: To improve UI organization for indicators with many parameters, making config windows less cluttered and more navigable
- **Location**: 
  - `src/ui/pine/settings_popover.rs`: Added `egui::collapsing_header::CollapsingState` for both Inputs and Style sections
  - `src/ui/kit/popover.rs`: Modified `panel_left()` to center popover on window instead of anchor

## Entry 6 - Dynamic Popover Sizing
- **Timestamp**: 2026-11-21 HH:MM
- **Change**: Made indicator settings popover dimensions relative to window size (60% width, 70% height) and centered on screen
- **Why**: To provide better UX across different monitor sizes and resolutions, ensuring popovers are neither too small nor off-screen
- **Location**: 
  - `src/ui/pine/settings_popover.rs`: Replaced fixed `WIDTH: 600.0` and `MAX_PAGE_HEIGHT: 480.0` with dynamic computation from `ctx.content_rect()`
  - `src/ui/kit/popover.rs`: Modified `panel_left()` to use `window.center().x` instead of `anchor.center().x` for centering

## Entry 7 - Yahoo Finance Integration (REST API + NSE Daily Data)
- **Timestamp**: 2026-11-21 HH:MM
- **Change**: Added `Yahoo` variant to `DataSource` enum with NSE stock support (`RELIANCE.NS`, `TCS.NS`, etc.), REST API integration pattern established, and daily local data update pipeline
- **Why**: Enable Indian stock market (NSE) data via Yahoo Finance REST API with daily persistence to local storage, avoiding live socket connections
- **Location**: `src/model/market/source.rs`: Added `Yahoo` enum variant (line 22), `display_name()` (line 41), `watchlist_slug()` (line 58), `provides_volume()` (lines 118-131), `display_symbol()` (lines 160-162), `base_asset()` (lines 178-179); `Yahoo` excluded from `CANDLE_STREAMS` and `QUOTE_STREAMS` (uses REST API only); `src/service/market/yahoo.rs`: New service file with `fetch_nse_candles()`, `normalize_nse_symbol()`, `YahooNseUpdater` struct for daily updates; `src/service/market/mod.rs`: Added `pub(in crate::service) mod yahoo;` (line 22)

## Entry 8 - Y-Axis Side & X-Axis Date Format Configuration + Compilation Fixes
- **Timestamp**: 2026-11-21 HH:MM
- **Change**: Added `y_axis_side` and `x_axis_format` configuration to charts, enabling users to position the y-axis on left or right side and set custom date format for x-axis labels; resolved 6 compilation errors related to enum defaults, field names, and format strings
- **Why**: Provide users with control over chart axis positioning and date formatting for better compatibility with regional preferences; fix build errors preventing compilation
- **Location**: 
  - `src/model/chart/appearance.rs`: Added `YAxisSide` enum (with `#[default]` on `Left`), `XAxisFormat` enum (with `#[default]` on `Default`), and `y_axis_side`/`x_axis_format` fields to `ChartAppearance`
  - `src/ui/chart/card/frame.rs`: Modified price axis drawing to respect `y_axis_side` (Left/Right positions)
  - `src/ui/chart/card/paint.rs`: Updated `draw_time_axis()` to accept `ChartAppearance` and use `x_axis_format` setting
  - `src/ui/chart/settings/appearance.rs`: Added `y_axis_side_selector()` and `x_axis_format_selector()` UI controls
  - `src/service/market/yahoo.rs`: Fixed `your_price source` → `your_price_source` field name; fixed format string in `fetch_nse_candles()`
  - `src/model/market/source.rs`: `provides_volume()` already includes `Self::Yahoo`; compilation errors resolved
  - `CHANGELOG_OPENAI.md`: Documents all changes with timestamps, reasons, and file locations

## Entry 9 - MStock Broker Integration
- **Timestamp**: 2026-11-21 HH:MM
- **Change**: Added `mstock` broker type integration with connection management, account handling, and order placement support
- **Why**: Enable support for mstock brokerage platform, extending the existing broker infrastructure to support additional trading venues
- **Location**: `src/host/broker/mod.rs`: Added `mstock` broker type enumeration and connection handling; `src/service/keychain/broker_secrets.rs`: Added mstock credential storage; `src/app/mod.rs`: BrokerHost initialization with mstock support; `src/workplace/trade/broker.rs`: mstock order placement logic
--