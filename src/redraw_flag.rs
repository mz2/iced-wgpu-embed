//! Shared redraw flag for coordinating iced redraw requests.
//!
//! [`RedrawFlag`] wraps an `Arc<AtomicBool>` that is shared between the
//! [`Notifier`](iced_wgpu::graphics::shell::Notifier) implementation (which
//! sets the flag when iced requests a redraw) and the render loop (which
//! checks and clears the flag each frame).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Shared redraw flag readable by both the notifier (owned by the iced Shell)
/// and the render loop.
#[derive(Clone)]
pub struct RedrawFlag(Arc<AtomicBool>);

impl RedrawFlag {
    /// Create a new redraw flag, initially set to `true` (request initial draw).
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(true)))
    }

    /// Check and clear the redraw flag. Returns the previous value.
    ///
    /// Called once per frame by the render loop. If `true`, iced has
    /// requested a redraw since the last frame.
    pub fn take(&self) -> bool {
        self.0.swap(false, Ordering::AcqRel)
    }

    /// Set the redraw flag (request a redraw).
    pub fn set(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// Get a clone of the inner `Arc<AtomicBool>` for constructing a
    /// custom notifier that shares this flag.
    pub fn inner(&self) -> Arc<AtomicBool> {
        self.0.clone()
    }
}

impl Default for RedrawFlag {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redraw_flag_starts_true() {
        let flag = RedrawFlag::new();
        assert!(flag.take());
    }

    #[test]
    fn redraw_flag_clears_after_take() {
        let flag = RedrawFlag::new();
        flag.take(); // clear initial
        assert!(!flag.take());
    }

    #[test]
    fn redraw_flag_set_and_take() {
        let flag = RedrawFlag::new();
        flag.take(); // clear initial
        flag.set();
        assert!(flag.take());
        assert!(!flag.take());
    }

    #[test]
    fn multiple_redraws_coalesce() {
        let flag = RedrawFlag::new();
        flag.take(); // clear initial
        flag.set();
        flag.set();
        flag.set();
        // Single take clears all
        assert!(flag.take());
        assert!(!flag.take());
    }
}
