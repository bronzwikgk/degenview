//! Durable, local OHLCV history. This is intentionally separate from the disposable network cache.

use crate::model::{DataSource, Kline, Timeframe};
use crate::store::Store;
use rusqlite::params;

impl Store {
    pub fn load_market_candles(
        &self,
        source: DataSource,
        symbol: &str,
        timeframe: Timeframe,
        limit: usize,
    ) -> rusqlite::Result<Vec<Kline>> {
        let mut statement = self.connection.prepare(
            "SELECT open_time, close_time, open, high, low, close, volume
             FROM market_candle WHERE source = ?1 AND symbol = ?2 AND timeframe = ?3
             ORDER BY open_time DESC LIMIT ?4",
        )?;
        let mut bars: Vec<Kline> = statement
            .query_map(
                params![
                    source.watchlist_slug(),
                    symbol,
                    timeframe.label(),
                    limit as i64
                ],
                |row| {
                    Ok(Kline {
                        open_time: row.get(0)?,
                        close_time: row.get(1)?,
                        open: row.get(2)?,
                        high: row.get(3)?,
                        low: row.get(4)?,
                        close: row.get(5)?,
                        volume: row.get(6)?,
                    })
                },
            )?
            .collect::<Result<_, _>>()?;
        bars.reverse();
        Ok(bars)
    }

    pub fn save_market_candles(
        &mut self,
        source: DataSource,
        symbol: &str,
        timeframe: Timeframe,
        bars: &[Kline],
        updated_at_ms: i64,
    ) -> rusqlite::Result<()> {
        let transaction = self.connection.transaction()?;
        {
            let mut insert = transaction.prepare(
                "INSERT INTO market_candle
                 (source, symbol, timeframe, open_time, close_time, open, high, low, close, volume, updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT(source, symbol, timeframe, open_time) DO UPDATE SET
                   close_time=excluded.close_time, open=excluded.open, high=excluded.high,
                   low=excluded.low, close=excluded.close, volume=excluded.volume,
                   updated_at_ms=excluded.updated_at_ms",
            )?;
            for bar in bars {
                insert.execute(params![
                    source.watchlist_slug(),
                    symbol,
                    timeframe.label(),
                    bar.open_time,
                    bar.close_time,
                    bar.open,
                    bar.high,
                    bar.low,
                    bar.close,
                    bar.volume,
                    updated_at_ms
                ])?;
            }
        }
        transaction.commit()
    }
}
