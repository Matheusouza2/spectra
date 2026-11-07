use racon_core::execution::ProgressUpdate;
use std::time::Instant;

/// Tracks progress of a long-running analysis.
pub struct ProgressTracker {
    start_time: Instant,
    current_phase: String,
}

impl ProgressTracker {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            current_phase: String::new(),
        }
    }

    /// Set the current phase label (e.g. "Building Reachability Graph").
    pub fn set_phase(&mut self, phase: String) {
        self.current_phase = phase;
    }

    /// Build a `ProgressUpdate` snapshot.
    pub fn build_update(
        &self,
        processed_nodes: usize,
        generated_nodes: usize,
        memory_mb: usize,
        active_threads: usize,
        progress: Option<f64>,
    ) -> ProgressUpdate {
        ProgressUpdate {
            phase: self.current_phase.clone(),
            processed_nodes,
            generated_nodes,
            memory_mb,
            elapsed_ms: self.start_time.elapsed().as_millis() as u64,
            active_threads,
            progress,
        }
    }
}

impl Default for ProgressTracker {
    fn default() -> Self {
        Self::new()
    }
}
