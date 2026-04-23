//! Performance diagnostics counters.
//!
//! [`Diagnostics`] provides lock-free, atomic counters for frame timing and
//! audio buffer underrun tracking. It is designed to be shared via `Arc`
//! between the render loop and any monitoring code (e.g. an overlay or
//! native-side telemetry).

use std::sync::atomic::{AtomicU64, Ordering};

/// Performance tracking counters (all atomic for lock-free access).
#[derive(Debug)]
pub struct Diagnostics {
    frame_time: AtomicU64,
    underruns: AtomicU64,
}

impl Diagnostics {
    /// Create a new diagnostics instance with all counters at zero.
    pub fn new() -> Self {
        Self {
            frame_time: AtomicU64::new(0),
            underruns: AtomicU64::new(0),
        }
    }

    /// Record the most recent frame render time in microseconds.
    pub fn record_frame_time_us(&self, us: u64) {
        self.frame_time.store(us, Ordering::Relaxed);
    }

    /// Read the most recent frame render time in microseconds.
    pub fn frame_time_us(&self) -> u64 {
        self.frame_time.load(Ordering::Relaxed)
    }

    /// Increment the cumulative audio buffer underrun count.
    pub fn increment_underruns(&self) {
        self.underruns.fetch_add(1, Ordering::Relaxed);
    }

    /// Read the cumulative audio buffer underrun count.
    pub fn underrun_count(&self) -> u64 {
        self.underruns.load(Ordering::Relaxed)
    }
}

impl Default for Diagnostics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_diagnostics_starts_at_zero() {
        let diag = Diagnostics::new();
        assert_eq!(diag.frame_time_us(), 0);
        assert_eq!(diag.underrun_count(), 0);
    }

    #[test]
    fn record_and_read_frame_time() {
        let diag = Diagnostics::new();
        diag.record_frame_time_us(16_666);
        assert_eq!(diag.frame_time_us(), 16_666);

        // Overwrites previous value
        diag.record_frame_time_us(8_333);
        assert_eq!(diag.frame_time_us(), 8_333);
    }

    #[test]
    fn increment_underruns_accumulates() {
        let diag = Diagnostics::new();
        diag.increment_underruns();
        diag.increment_underruns();
        diag.increment_underruns();
        assert_eq!(diag.underrun_count(), 3);
    }

    #[test]
    fn default_is_same_as_new() {
        let diag = Diagnostics::default();
        assert_eq!(diag.frame_time_us(), 0);
        assert_eq!(diag.underrun_count(), 0);
    }
}
