//! Notifier implementations for bridging iced redraw requests.
//!
//! When iced's internal state changes (e.g. a widget animation or layout
//! invalidation), it calls [`Notifier::request_redraw`] on the notifier
//! owned by the [`Shell`](iced_wgpu::graphics::shell::Shell). These
//! implementations translate that signal into something the platform's
//! render loop can observe.
//!
//! Two implementations are provided:
//!
//! - [`SimpleNotifier`] — sets an atomic flag only. Suitable for platforms
//!   where the render loop polls each frame (e.g. Android Choreographer).
//!
//! - [`CallbackNotifier`] — sets the atomic flag **and** calls a user-provided
//!   closure. Suitable for platforms that need an explicit wake-up signal
//!   (e.g. iOS CADisplayLink that may be paused).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use iced_wgpu::graphics::shell::Notifier;

use crate::RedrawFlag;

/// A simple notifier that only sets an atomic redraw flag.
///
/// Use this on platforms where the render loop runs continuously or is
/// driven by an external vsync callback (e.g. Android's `Choreographer`).
/// The render loop checks the flag via [`RedrawFlag::take`] each frame.
///
/// # Example
///
/// ```ignore
/// let redraw_flag = RedrawFlag::new();
/// let notifier = SimpleNotifier::new(&redraw_flag);
/// // Pass `notifier` to IcedEmbed::new(), keep `redraw_flag` for the render loop
/// ```
pub struct SimpleNotifier {
    redraw_requested: Arc<AtomicBool>,
}

impl SimpleNotifier {
    /// Create a notifier that shares the given [`RedrawFlag`].
    pub fn new(flag: &RedrawFlag) -> Self {
        Self {
            redraw_requested: flag.inner(),
        }
    }
}

impl Notifier for SimpleNotifier {
    fn tick(&self) {
        // No-op — subscriptions are not used in embedded mode
    }

    fn request_redraw(&self) {
        self.redraw_requested.store(true, Ordering::Release);
    }

    fn invalidate_layout(&self) {
        self.request_redraw();
    }
}

/// A notifier that sets the atomic flag **and** calls a closure on redraw.
///
/// Use this on platforms that need an explicit wake-up signal when iced
/// requests a redraw. For example, on iOS you might resume a paused
/// `CADisplayLink`:
///
/// ```ignore
/// let redraw_flag = RedrawFlag::new();
/// let notifier = CallbackNotifier::new(&redraw_flag, || {
///     // Call back into Swift to resume CADisplayLink
///     (swift_callback)(CallbackType::RequestRedraw, std::ptr::null(), 0);
/// });
/// ```
pub struct CallbackNotifier<F: Fn() + Send + Sync> {
    redraw_requested: Arc<AtomicBool>,
    on_redraw: F,
}

impl<F: Fn() + Send + Sync> CallbackNotifier<F> {
    /// Create a notifier that shares the given [`RedrawFlag`] and calls
    /// `on_redraw` whenever iced requests a redraw.
    pub fn new(flag: &RedrawFlag, on_redraw: F) -> Self {
        Self {
            redraw_requested: flag.inner(),
            on_redraw,
        }
    }
}

impl<F: Fn() + Send + Sync + 'static> Notifier for CallbackNotifier<F> {
    fn tick(&self) {
        // No-op — subscriptions are not used in embedded mode
    }

    fn request_redraw(&self) {
        self.redraw_requested.store(true, Ordering::Release);
        (self.on_redraw)();
    }

    fn invalidate_layout(&self) {
        self.request_redraw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_notifier_request_redraw_sets_flag() {
        let flag = RedrawFlag::new();
        flag.take(); // clear initial
        let notifier = SimpleNotifier::new(&flag);

        notifier.request_redraw();
        assert!(flag.take());
        assert!(!flag.take()); // cleared
    }

    #[test]
    fn simple_notifier_invalidate_layout_sets_flag() {
        let flag = RedrawFlag::new();
        flag.take(); // clear initial
        let notifier = SimpleNotifier::new(&flag);

        notifier.invalidate_layout();
        assert!(flag.take());
    }

    #[test]
    fn callback_notifier_calls_callback_on_redraw() {
        use std::sync::atomic::AtomicUsize;

        let flag = RedrawFlag::new();
        flag.take(); // clear initial
        let call_count = Arc::new(AtomicUsize::new(0));
        let count_clone = call_count.clone();
        let notifier = CallbackNotifier::new(&flag, move || {
            count_clone.fetch_add(1, Ordering::Relaxed);
        });

        notifier.request_redraw();
        assert!(flag.take());
        assert_eq!(call_count.load(Ordering::Relaxed), 1);

        notifier.request_redraw();
        assert_eq!(call_count.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn callback_notifier_invalidate_layout_calls_callback() {
        use std::sync::atomic::AtomicUsize;

        let flag = RedrawFlag::new();
        flag.take(); // clear initial
        let call_count = Arc::new(AtomicUsize::new(0));
        let count_clone = call_count.clone();
        let notifier = CallbackNotifier::new(&flag, move || {
            count_clone.fetch_add(1, Ordering::Relaxed);
        });

        notifier.invalidate_layout();
        assert!(flag.take());
        assert_eq!(call_count.load(Ordering::Relaxed), 1);
    }
}
