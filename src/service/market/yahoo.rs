//! Yahoo Finance REST market-data provider.
//!
//! Yahoo has no candle stream in DegenView: chart updates are obtained through
//! normal REST refreshes. Yahoo chart timestamps are seconds; DegenView uses
//! milliseconds, so conversion happens at this boundary.

use super::candles::plan;
use crate::model::market::bucket::{bucket_start_ms, next_bucket_start_ms};
use crate::model::{DataSource, Kline, Timeframe};
use crate::service::{rank_results, MarketSource, ProviderError, SearchResult};
use crate::store::SharedStore;
use serde_json::Value;
use std::sync::OnceLock;

const CHART_URL: &str = "https://query1.finance.yahoo.com/v8/finance/chart";
const SEARCH_URL: &str = "https://query1.finance.yahoo.com/v1/finance/search";
const MAX_BARS: usize = 10_000;
const YAHOO_HEADERS: [(&str, &str); 2] = [
    ("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"),
    ("Accept", "*/*"),
];

pub struct Yahoo;
pub const YAHOO: Yahoo = Yahoo;

static STORE: OnceLock<SharedStore> = OnceLock::new();

/// Installs the app's durable store before Yahoo charts start fetching.
pub fn install_store(store: SharedStore) {
    let _ = STORE.set(store);
}

impl MarketSource for Yahoo {
    fn fetch_klines(
        &self,
        symbol: &str,
        timeframe: Timeframe,
        limit: u32,
    ) -> Result<Vec<Kline>, ProviderError> {
        let plan = plan(DataSource::Yahoo, timeframe)?;
        let requested = plan.base_count(limit as usize).min(MAX_BARS);
        let since = crate::clock::now_ms() - requested as i64 * plan.base().nominal_ms() * 3;
        let local = load_local(symbol, plan.base(), requested);
        // A newly opened daily Yahoo market is backfilled once from the provider's first
        // available candle. Subsequent chart loads only refresh the recent request window.
        let initial_backfill = local.is_empty() && plan.base() == Timeframe::DAY_1;
        let start = initial_backfill.then_some(0).or(Some(since));
        let bars = match fetch_bars(symbol, plan.base(), requested, start, None) {
            Ok(bars) => {
                save_local(symbol, plan.base(), &bars);
                bars
            }
            Err(error) if !local.is_empty() => local,
            Err(error) => return Err(error),
        };
        Ok(plan.shape_since(bars, requested, since, limit as usize))
    }

    fn fetch_klines_range(
        &self,
        symbol: &str,
        timeframe: Timeframe,
        start_ms: i64,
        end_ms: i64,
        limit: usize,
    ) -> Result<Vec<Kline>, ProviderError> {
        let plan = plan(DataSource::Yahoo, timeframe)?;
        let requested = plan.base_count(limit).min(MAX_BARS);
        let bars = fetch_bars(symbol, plan.base(), requested, Some(start_ms), Some(end_ms))?;
        save_local(symbol, plan.base(), &bars);
        Ok(plan.shape_since(bars, requested, start_ms, limit))
    }

    fn search_tickers(&self, query: &str) -> Result<Vec<SearchResult>, ProviderError> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let url = format!(
            "{SEARCH_URL}?q={}&quotesCount=20&newsCount=0",
            url_encode(query)
        );
        let body = crate::service::net::http::get_text(
            &url,
            &YAHOO_HEADERS,
            crate::service::net::http::Retry::Transient,
        )
        .map_err(ProviderError::from)?;
        let payload: Value = serde_json::from_str(&body).map_err(ProviderError::data)?;
        let mut results = Vec::new();
        for quote in payload["quotes"].as_array().into_iter().flatten() {
            let symbol = quote["symbol"].as_str().unwrap_or_default();
            let name = quote["longname"]
                .as_str()
                .or_else(|| quote["shortname"].as_str())
                .unwrap_or(symbol);
            let exchange = quote["exchange"].as_str().unwrap_or_default();
            if symbol.ends_with(".NS") || exchange == "NSI" || exchange == "NSE" {
                results.push(SearchResult::new(
                    DataSource::Yahoo,
                    format!("{symbol}{}{}", crate::service::NAME_SEPARATOR, name),
                    symbol,
                ));
            }
        }
        Ok(rank_results(results, query))
    }
}

fn fetch_bars(
    symbol: &str,
    timeframe: Timeframe,
    limit: usize,
    start_ms: Option<i64>,
    end_ms: Option<i64>,
) -> Result<Vec<Kline>, ProviderError> {
    let interval = yahoo_interval(timeframe)?;
    let mut url = format!(
        "{CHART_URL}/{}?interval={interval}&events=history",
        url_encode(symbol.trim())
    );
    if start_ms == Some(0) && end_ms.is_none() {
        // Yahoo rejects `period1=0`; its documented `range=max` means the earliest history it
        // makes available for the requested interval.
        url.push_str("&range=max");
    } else if let Some(start_ms) = start_ms {
        url.push_str(&format!("&period1={}", start_ms.div_euclid(1_000)));
        if let Some(end_ms) = end_ms {
            url.push_str(&format!("&period2={}", end_ms.div_euclid(1_000)));
        }
    } else {
        let calendar_days = match timeframe.unit() {
            crate::model::TimeUnit::Day => limit.saturating_mul(timeframe.count() as usize + 3),
            crate::model::TimeUnit::Week => limit.saturating_mul(timeframe.count() as usize * 8),
            crate::model::TimeUnit::Month => limit.saturating_mul(timeframe.count() as usize * 35),
            _ => return Err(ProviderError::unsupported(DataSource::Yahoo, timeframe)),
        };
        url.push_str(&format!(
            "&period1={}",
            now_seconds() - calendar_days as i64 * 86_400
        ));
    }
    let body =
        crate::service::net::http::get_text(&url, &YAHOO_HEADERS, crate::service::net::http::Retry::Transient)
            .map_err(ProviderError::from)?;
    parse_bars(&body, timeframe)
}

fn yahoo_interval(timeframe: Timeframe) -> Result<&'static str, ProviderError> {
    use crate::model::TimeUnit;
    match (timeframe.count(), timeframe.unit()) {
        (1, TimeUnit::Day) => Ok("1d"),
        (1, TimeUnit::Week) => Ok("1wk"),
        (1, TimeUnit::Month) => Ok("1mo"),
        _ => Err(ProviderError::unsupported(DataSource::Yahoo, timeframe)),
    }
}

fn parse_bars(body: &str, timeframe: Timeframe) -> Result<Vec<Kline>, ProviderError> {
    let payload: Value = serde_json::from_str(body).map_err(ProviderError::data)?;
    if let Some(description) = payload["chart"]["error"]["description"].as_str() {
        return Err(ProviderError::other(format!(
            "Yahoo Finance: {description}"
        )));
    }
    let result = payload["chart"]["result"]
        .as_array()
        .and_then(|results| results.first())
        .ok_or_else(|| ProviderError::other("Yahoo Finance returned no chart data"))?;
    let timestamps = result["timestamp"]
        .as_array()
        .ok_or_else(|| ProviderError::data("Yahoo Finance response has no timestamps"))?;
    let quote = result["indicators"]["quote"]
        .as_array()
        .and_then(|quotes| quotes.first())
        .ok_or_else(|| ProviderError::data("Yahoo Finance response has no OHLC data"))?;
    let empty = Vec::new();
    let opens = quote["open"].as_array().unwrap_or(&empty);
    let highs = quote["high"].as_array().unwrap_or(&empty);
    let lows = quote["low"].as_array().unwrap_or(&empty);
    let closes = quote["close"].as_array().unwrap_or(&empty);
    let volumes = quote["volume"].as_array().unwrap_or(&empty);
    let mut bars = Vec::with_capacity(timestamps.len());
    for index in 0..timestamps.len() {
        let Some(timestamp) = timestamps[index].as_i64() else {
            continue;
        };
        let (Some(open), Some(high), Some(low), Some(close)) = (
            opens.get(index).and_then(Value::as_f64),
            highs.get(index).and_then(Value::as_f64),
            lows.get(index).and_then(Value::as_f64),
            closes.get(index).and_then(Value::as_f64),
        ) else {
            continue;
        };
        if !open.is_finite() || !high.is_finite() || !low.is_finite() || !close.is_finite() {
            continue;
        }
        let open_time = bucket_start_ms(timeframe, timestamp * 1_000);
        bars.push(Kline {
            open_time,
            close_time: next_bucket_start_ms(timeframe, open_time) - 1,
            open,
            high,
            low,
            close,
            volume: volumes
                .get(index)
                .and_then(Value::as_f64)
                .unwrap_or_default(),
        });
    }
    if bars.is_empty() {
        return Err(ProviderError::other(
            "Yahoo Finance returned no usable OHLC bars",
        ));
    }
    Ok(bars)
}

fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}

fn load_local(symbol: &str, timeframe: Timeframe, limit: usize) -> Vec<Kline> {
    STORE
        .get()
        .and_then(|store| store.lock().ok())
        .and_then(|store| {
            store
                .load_market_candles(DataSource::Yahoo, symbol, timeframe, limit)
                .ok()
        })
        .unwrap_or_default()
}

fn save_local(symbol: &str, timeframe: Timeframe, bars: &[Kline]) {
    if let Some(mut store) = STORE.get().and_then(|store| store.lock().ok()) {
        let _ = store.save_market_candles(
            DataSource::Yahoo,
            symbol,
            timeframe,
            bars,
            crate::clock::now_ms(),
        );
    }
}

fn url_encode(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'-' | b'_' => vec![byte as char],
            _ => format!("%{byte:02X}").chars().collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{TimeUnit, Timeframe};
    #[test]
    fn encodes_nse_symbol_without_changing_dot() {
        assert_eq!(url_encode("RELIANCE.NS"), "RELIANCE.NS");
        assert_eq!(url_encode("M&M.NS"), "M%26M.NS");
    }
    #[test]
    fn yahoo_supports_daily_weekly_and_monthly_only() {
        assert_eq!(
            yahoo_interval(Timeframe::new(1, TimeUnit::Day).unwrap()).unwrap(),
            "1d"
        );
        assert!(yahoo_interval(Timeframe::new(4, TimeUnit::Hour).unwrap()).is_err());
    }
}
