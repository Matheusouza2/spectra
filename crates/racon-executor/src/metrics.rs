use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

/// Collects performance counters and timing data during analysis.
pub struct MetricsCollector {
    start_time: Instant,
    counters: Mutex<HashMap<String, AtomicU64>>,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            counters: Mutex::new(HashMap::new()),
        }
    }

    /// Increment a named counter by 1.
    pub fn increment(&self, counter: &str) {
        let mut counters = self.counters.lock().unwrap();
        counters
            .entry(counter.to_string())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Milliseconds elapsed since this collector was created.
    pub fn elapsed_ms(&self) -> u64 {
        self.start_time.elapsed().as_millis() as u64
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}
