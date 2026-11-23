//! Local OHLCV CSV market data provider.
//!
//! Reads per-symbol daily CSV files (`Date,Open,High,Low,Close,Volume`) from the directory
//! configured by `DEGENVIEW_LOCAL_DATA_DIR`, defaulting to `E:\data\ohlcv\NSE`.

use super::candles::plan;
use crate::model::watchlist::quote::{SourceQuote, VolumeKind};
use crate::model::{DataSource, Kline, Timeframe};
use crate::service::{MarketSource, ProviderError, SearchResult, rank_results};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const DAY_MS: i64 = 86_400_000;
const CATALOG_TTL: Duration = Duration::from_secs(30);
const DEFAULT_DIRS: [&str; 2] = [r"E:\data\ohlcv\NSE", r"data\ohlcv\NSE"];

pub struct LocalCsv;

pub const LOCAL_CSV: LocalCsv = LocalCsv;

/// Directory containing `<SYMBOL>.csv` files.
pub fn data_dir() -> PathBuf {
    if let Ok(custom) = std::env::var("DEGENVIEW_LOCAL_DATA_DIR") {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    for candidate in DEFAULT_DIRS {
        let path = PathBuf::from(candidate);
        if path.is_dir() {
            return path;
        }
    }
    PathBuf::from(DEFAULT_DIRS[0])
}

struct SymbolIndex {
    dir: PathBuf,
    scanned_at: Instant,
    /// Upper-cased symbol -> path to its CSV file.
    files: HashMap<String, PathBuf>,
    /// Sorted list of upper-cased symbols for deterministic browsing/search.
    symbols: Vec<String>,
}

fn index_cache() -> &'static Mutex<Option<SymbolIndex>> {
    static CACHE: OnceLock<Mutex<Option<SymbolIndex>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

fn scan_dir(dir: &Path) -> Result<SymbolIndex, ProviderError> {
    let entries = fs::read_dir(dir).map_err(|error| {
        ProviderError::Other(format!(
            "Cannot read local data folder {}: {error}",
            dir.display()
        ))
    })?;
    let mut files = HashMap::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let is_csv = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("csv"));
        if !is_csv {
            continue;
        }
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            let symbol = stem.trim().to_uppercase();
            if !symbol.is_empty() {
                files.insert(symbol, path);
            }
        }
    }
    let mut symbols: Vec<String> = files.keys().cloned().collect();
    symbols.sort();
    Ok(SymbolIndex {
        dir: dir.to_path_buf(),
        scanned_at: Instant::now(),
        files,
        symbols,
    })
}

fn with_index<T>(f: impl FnOnce(&SymbolIndex) -> T) -> Result<T, ProviderError> {
    let dir = data_dir();
    let mut guard = index_cache()
        .lock()
        .map_err(|_| ProviderError::other("Local symbol index unavailable"))?;
    let needs_scan = match guard.as_ref() {
        Some(idx) => idx.dir != dir || idx.scanned_at.elapsed() >= CATALOG_TTL,
        None => true,
    };
    if needs_scan {
        *guard = Some(scan_dir(&dir)?);
    }
    Ok(f(guard.as_ref().expect("just populated")))
}

/// Popular NSE tickers shown first when the picker has no query yet, filtered to ones present on disk.
const FEATURED_SYMBOLS: [&str; 12] = [
    "RELIANCE",
    "TCS",
    "HDFCBANK",
    "INFY",
    "ICICIBANK",
    "SBIN",
    "BHARTIARTL",
    "ITC",
    "LT",
    "BAJFINANCE",
    "HINDUNILVR",
    "TITAN",
];

pub fn featured_results() -> Vec<SearchResult> {
    with_index(|idx| {
        let mut results: Vec<SearchResult> = FEATURED_SYMBOLS
            .iter()
            .filter(|sym| idx.files.contains_key(**sym))
            .map(|sym| SearchResult::new(DataSource::Local, *sym, *sym))
            .collect();
        if results.is_empty() {
            results = idx
                .symbols
                .iter()
                .take(12)
                .map(|sym| SearchResult::new(DataSource::Local, sym.clone(), sym.clone()))
                .collect();
        }
        results
    })
    .unwrap_or_default()
}

fn resolve_csv_path(symbol: &str) -> Result<PathBuf, ProviderError> {
    let key = symbol.trim().to_uppercase();
    if key.is_empty() {
        return Err(ProviderError::other("Empty local symbol"));
    }
    // Direct path check avoids rescanning if the file is freshly added.
    let direct = data_dir().join(format!("{key}.csv"));
    if direct.is_file() {
        return Ok(direct);
    }
    with_index(|idx| idx.files.get(&key).cloned())?
        .ok_or_else(|| ProviderError::Other(format!("Local CSV not found for {key}")))
}

/// Parses `YYYY-MM-DD`, `YYYY-MM-DD[T ]HH:MM:SS...`, or a Unix timestamp (seconds or ms) into epoch ms.
fn parse_timestamp_ms(raw: &str) -> Option<i64> {
    let text = raw.trim().trim_matches('"');
    if text.is_empty() {
        return None;
    }
    if text.len() >= 10 && text.as_bytes()[4] == b'-' && text.as_bytes()[7] == b'-' {
        let year: i64 = text[0..4].parse().ok()?;
        let month: i64 = text[5..7].parse().ok()?;
        let day: i64 = text[8..10].parse().ok()?;
        if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
            return None;
        }
        let mut ms = crate::format::days_from_civil(year, month, day) * DAY_MS;
        if text.len() >= 19
            && (text.as_bytes()[10] == b'T' || text.as_bytes()[10] == b' ')
            && text.as_bytes()[13] == b':'
            && text.as_bytes()[16] == b':'
        {
            let hour: i64 = text[11..13].parse().ok()?;
            let minute: i64 = text[14..16].parse().ok()?;
            let second: i64 = text[17..19].parse().ok()?;
            ms += ((hour * 60 + minute) * 60 + second) * 1_000;
        }
        return Some(ms);
    }
    let numeric: i64 = text.parse().ok()?;
    if numeric.abs() < 10_000_000_000 {
        Some(numeric * 1_000)
    } else {
        Some(numeric)
    }
}

fn parse_csv_line(line: &str) -> Option<Kline> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut parts = trimmed.split(',');
    let open_time = parse_timestamp_ms(parts.next()?)?;
    let open: f64 = parts.next()?.trim().parse().ok()?;
    let high: f64 = parts.next()?.trim().parse().ok()?;
    let low: f64 = parts.next()?.trim().parse().ok()?;
    let close: f64 = parts.next()?.trim().parse().ok()?;
    let volume: f64 = parts
        .next()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0.0);
    if ![open, high, low, close].iter().all(|v| v.is_finite()) {
        return None;
    }
    Some(Kline {
        open_time,
        close_time: open_time + DAY_MS - 1,
        open,
        high,
        low,
        close,
        volume: if volume.is_finite() { volume } else { 0.0 },
    })
}

fn load_daily_bars(path: &Path) -> Result<Vec<Kline>, ProviderError> {
    let file = File::open(path).map_err(|error| {
        ProviderError::Other(format!("Failed to open {}: {error}", path.display()))
    })?;
    let reader = BufReader::new(file);
    let mut bars = Vec::with_capacity(1024);
    for line in reader.lines() {
        let line = line.map_err(|error| {
            ProviderError::Other(format!("Failed reading {}: {error}", path.display()))
        })?;
        if let Some(bar) = parse_csv_line(&line) {
            bars.push(bar);
        }
    }
    bars.sort_by_key(|bar| bar.open_time);
    Ok(bars)
}

/// Fast watchlist quote: reads only the last few KB of the CSV to get the latest 2 daily closes.
pub fn quote_for_symbol(symbol: &str) -> Option<SourceQuote> {
    let path = resolve_csv_path(symbol).ok()?;
    let mut file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let tail_bytes: u64 = 4096;
    if len > tail_bytes {
        file.seek(SeekFrom::End(-(tail_bytes as i64))).ok()?;
    }
    let mut buf = String::new();
    file.read_to_string(&mut buf).ok()?;
    let mut parsed: Vec<Kline> = buf.lines().filter_map(parse_csv_line).collect();
    let last = parsed.pop()?;
    let prev = parsed.pop();
    Some(SourceQuote {
        price: last.close,
        previous_day_price: prev.map(|bar| bar.close),
        volume_24h: Some(last.volume),
        volume_kind: Some(VolumeKind::Shares),
        timestamp_ms: Some(last.open_time),
    })
}

impl MarketSource for LocalCsv {
    fn fetch_klines(
        &self,
        symbol: &str,
        timeframe: Timeframe,
        limit: u32,
    ) -> Result<Vec<Kline>, ProviderError> {
        let plan = plan(DataSource::Local, timeframe)?;
        let path = resolve_csv_path(symbol)?;
        let mut bars = load_daily_bars(&path)?;
        if bars.is_empty() {
            return Err(ProviderError::Other(format!(
                "No valid OHLCV rows found in {}",
                path.display()
            )));
        }
        let requested = plan.base_count(limit as usize);
        if bars.len() > requested {
            bars.drain(..bars.len() - requested);
        }
        Ok(plan.shape(bars, requested, limit as usize))
    }

    fn fetch_klines_range(
        &self,
        symbol: &str,
        timeframe: Timeframe,
        start_ms: i64,
        end_ms: i64,
        max: usize,
    ) -> Result<Vec<Kline>, ProviderError> {
        let plan = plan(DataSource::Local, timeframe)?;
        let path = resolve_csv_path(symbol)?;
        let mut bars: Vec<Kline> = load_daily_bars(&path)?
            .into_iter()
            .filter(|bar| bar.open_time >= start_ms && bar.open_time < end_ms)
            .collect();
        let requested = plan.base_count(max);
        if bars.len() > requested {
            bars.drain(..bars.len() - requested);
        }
        Ok(plan.shape(bars, requested, max))
    }

    fn search_tickers(&self, query: &str) -> Result<Vec<SearchResult>, ProviderError> {
        let needle = query.trim().to_uppercase();
        if needle.is_empty() {
            return Ok(featured_results());
        }
        with_index(|idx| {
            let results: Vec<SearchResult> = idx
                .symbols
                .iter()
                .filter(|sym| sym.contains(&needle))
                .map(|sym| SearchResult::new(DataSource::Local, sym.clone(), sym.clone()))
                .collect();
            rank_results(results, query).into_iter().take(50).collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nse_daily_csv_rows() {
        assert!(parse_csv_line("Date,Open,High,Low,Close,Volume").is_none());
        let bar = parse_csv_line("2026-11-17,1339,1374,1335,1367,15861037").unwrap();
        assert_eq!(
            bar.open_time,
            crate::format::days_from_civil(2026, 11, 17) * DAY_MS
        );
        assert_eq!(bar.open, 1339.0);
        assert_eq!(bar.high, 1374.0);
        assert_eq!(bar.low, 1335.0);
        assert_eq!(bar.close, 1367.0);
        assert_eq!(bar.volume, 15_861_037.0);
    }

    #[test]
    fn loads_and_folds_daily_bars_to_weekly() {
        let dir = std::env::temp_dir().join("degenview_local_csv_test");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("TESTSYM.csv");
        let csv = "\
Date,Open,High,Low,Close,Volume
2026-11-02,100,105,99,104,1000
2026-11-03,104,108,103,107,1200
2026-11-04,107,110,106,109,1500
2026-11-09,110,115,109,114,2000
2026-11-10,114,116,112,115,1800
";
        fs::write(&path, csv).unwrap();
        let bars = load_daily_bars(&path).unwrap();
        assert_eq!(bars.len(), 5);
        let plan = plan(DataSource::Local, Timeframe::WEEK_1).unwrap();
        let weekly = plan.shape(bars, 50, 10);
        assert_eq!(weekly.len(), 2);
        assert_eq!(weekly[0].open, 100.0);
        assert_eq!(weekly[0].high, 110.0);
        assert_eq!(weekly[0].close, 109.0);
        assert_eq!(weekly[1].open, 110.0);
        assert_eq!(weekly[1].close, 115.0);
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);
    }
}
