/// In-memory OHLCV history store for the price feed (Issue #119).
///
/// Each token maintains a ring buffer of (timestamp_secs, price) ticks capped
/// at `MAX_TICKS` entries (1440 = 24 h × 60 min at 1-minute granularity).
/// OHLCV aggregation and forward-fill happen at query time.
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use serde::Serialize;

/// Maximum ticks retained per token — 24 h of 1-minute data.
pub const MAX_TICKS: usize = 1440;

/// Supported candle intervals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interval {
    OneMinute,
    FiveMinutes,
    OneHour,
}

impl Interval {
    /// Duration in seconds.
    pub fn secs(self) -> u64 {
        match self {
            Self::OneMinute => 60,
            Self::FiveMinutes => 300,
            Self::OneHour => 3600,
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "1m" => Some(Self::OneMinute),
            "5m" => Some(Self::FiveMinutes),
            "1h" => Some(Self::OneHour),
            _ => None,
        }
    }
}

/// A single OHLCV candle.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Candle {
    /// Unix timestamp (seconds) of the candle's open.
    pub timestamp: u64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    /// Number of ticks that fell in this window (proxy for volume).
    pub volume: u64,
}

/// Shared, thread-safe history store.
#[derive(Clone)]
pub struct HistoryStore {
    inner: Arc<Mutex<HashMap<String, VecDeque<(u64, f64)>>>>,
}

impl HistoryStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Record a price tick for a token.
    pub fn record(&self, token: &str, timestamp_secs: u64, price: f64) {
        let mut map = self.inner.lock().unwrap();
        let buf = map.entry(token.to_lowercase()).or_default();
        buf.push_back((timestamp_secs, price));
        while buf.len() > MAX_TICKS {
            buf.pop_front();
        }
    }

    /// Query OHLCV candles for a token between `from` and `to` (inclusive,
    /// Unix seconds) at the given interval.  Missing intervals are forward-filled
    /// from the last known close.
    ///
    /// Returns `None` if the token has no history at all.
    pub fn query(
        &self,
        token: &str,
        from: u64,
        to: u64,
        interval: Interval,
    ) -> Option<Vec<Candle>> {
        if from > to {
            return Some(vec![]);
        }
        let map = self.inner.lock().unwrap();
        let buf = map.get(&token.to_lowercase())?;

        let step = interval.secs();
        // Align `from` to the nearest bucket boundary.
        let bucket_start = (from / step) * step;

        let ticks: Vec<(u64, f64)> = buf
            .iter()
            .filter(|(ts, _)| *ts >= from && *ts <= to)
            .cloned()
            .collect();

        let mut candles: Vec<Candle> = Vec::new();
        let mut last_close: Option<f64> = buf
            .iter()
            .filter(|(ts, _)| *ts < from)
            .last()
            .map(|(_, p)| *p);

        let mut bucket = bucket_start;
        while bucket <= to {
            let bucket_end = bucket + step;
            let window: Vec<f64> = ticks
                .iter()
                .filter(|(ts, _)| *ts >= bucket && *ts < bucket_end)
                .map(|(_, p)| *p)
                .collect();

            if window.is_empty() {
                // Forward-fill from last close.
                if let Some(fill) = last_close {
                    candles.push(Candle {
                        timestamp: bucket,
                        open: fill,
                        high: fill,
                        low: fill,
                        close: fill,
                        volume: 0,
                    });
                }
            } else {
                let open = window[0];
                let close = *window.last().unwrap();
                let high = window.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                let low = window.iter().cloned().fold(f64::INFINITY, f64::min);
                last_close = Some(close);
                candles.push(Candle {
                    timestamp: bucket,
                    open,
                    high,
                    low,
                    close,
                    volume: window.len() as u64,
                });
            }

            bucket += step;
        }

        Some(candles)
    }
}

impl Default for HistoryStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Insert 60 1-minute prices and query as 5-minute candles (12 candles expected).
    #[test]
    fn test_60_min_ticks_as_5m_candles() {
        let store = HistoryStore::new();
        let base_ts: u64 = 1_700_000_000; // arbitrary epoch

        for i in 0..60u64 {
            let ts = base_ts + i * 60;
            let price = 100.0 + i as f64; // 100, 101, ..., 159
            store.record("btc", ts, price);
        }

        let from = base_ts;
        let to = base_ts + 59 * 60;
        let candles = store
            .query("btc", from, to, Interval::FiveMinutes)
            .expect("should have history");

        // 60 minutes / 5 = 12 candles
        assert_eq!(candles.len(), 12, "expected 12 five-minute candles");

        // First candle: ticks 0..4 → prices 100..104
        let c0 = &candles[0];
        assert_eq!(c0.open, 100.0);
        assert_eq!(c0.close, 104.0);
        assert_eq!(c0.high, 104.0);
        assert_eq!(c0.low, 100.0);
        assert_eq!(c0.volume, 5);

        // Each candle covers 5 ticks
        for c in &candles {
            assert!(c.volume == 5 || c.volume == 0);
        }
    }

    #[test]
    fn test_missing_intervals_forward_filled() {
        let store = HistoryStore::new();
        let base: u64 = 1_700_000_000;

        // Only tick at t=0 and t=600 (i.e., minute 0 and minute 10)
        store.record("eth", base, 200.0);
        store.record("eth", base + 600, 210.0);

        let candles = store
            .query("eth", base, base + 599, Interval::FiveMinutes)
            .unwrap();

        // Two 5-minute windows: [base, base+300) and [base+300, base+600)
        assert_eq!(candles.len(), 2);
        // First window has one tick
        assert_eq!(candles[0].open, 200.0);
        assert_eq!(candles[0].volume, 1);
        // Second window is empty → forward-fill at 200.0
        assert_eq!(candles[1].open, 200.0);
        assert_eq!(candles[1].volume, 0);
    }

    #[test]
    fn test_ring_buffer_capped_at_max_ticks() {
        let store = HistoryStore::new();
        for i in 0..(MAX_TICKS + 10) as u64 {
            store.record("sol", i * 60, 50.0);
        }
        let map = store.inner.lock().unwrap();
        assert_eq!(map["sol"].len(), MAX_TICKS);
    }

    #[test]
    fn test_unknown_token_returns_none() {
        let store = HistoryStore::new();
        let result = store.query("unknown", 0, 3600, Interval::OneHour);
        assert!(result.is_none());
    }

    #[test]
    fn test_interval_seconds() {
        assert_eq!(Interval::OneMinute.secs(), 60);
        assert_eq!(Interval::FiveMinutes.secs(), 300);
        assert_eq!(Interval::OneHour.secs(), 3600);
    }
}
