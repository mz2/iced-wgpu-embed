//! The [`Program`] trait for embedded iced applications.
//!
//! This extends iced's core `Program` concept (Message + update + view) with
//! lifecycle hooks specific to native embedding: per-frame ticking, viewport
//! changes, serialization, and frame-bracketing hooks.
//!
//! Implementors provide the application logic; [`IcedEmbed`](crate::IcedEmbed)
//! handles all wgpu and iced rendering infrastructure.
//!
//! # Example
//!
//! ```ignore
//! use iced_wgpu_embed::Program;
//! use iced_core::Element;
//! use iced_widget::{text, Theme};
//! use iced_wgpu::Renderer;
//!
//! struct Counter { count: i32 }
//!
//! #[derive(Clone, Debug)]
//! enum Message { Tick, Increment }
//!
//! impl Program for Counter {
//!     type Message = Message;
//!
//!     fn update(&mut self, message: Message) {
//!         match message {
//!             Message::Tick => {}
//!             Message::Increment => self.count += 1,
//!         }
//!     }
//!
//!     fn view(&self) -> Element<'_, Message, Theme, Renderer> {
//!         text(format!("Count: {}", self.count)).into()
//!     }
//!
//!     fn tick_message() -> Option<Message> { Some(Message::Tick) }
//!     fn set_viewport_size(&mut self, _w: f32, _h: f32) {}
//!     fn to_json(&self) -> String { format!("{{\"count\":{}}}", self.count) }
//! }
//! ```

use iced_core::Element;
use iced_wgpu::Renderer;
use iced_widget::Theme;

/// An application that can be embedded into a native view hierarchy via
/// [`IcedEmbed`](crate::IcedEmbed).
///
/// This trait combines iced's core program interface (message, update, view)
/// with lifecycle hooks for native embedding:
///
/// - **Per-frame tick**: Each frame, [`IcedEmbed`](crate::IcedEmbed) sends
///   [`tick_message`](Program::tick_message) to the app before building the UI.
///   Use this to poll external state (audio feedback, network, etc.).
///
/// - **Lifecycle hooks**: [`pre_frame`](Program::pre_frame) and
///   [`post_update`](Program::post_update) bracket the render loop for
///   platform-specific work (reading audio atomics, flushing event queues).
///
/// - **Serialization**: [`to_json`](Program::to_json) allows the platform to
///   persist app state on background/terminate.
///
/// The program is constructed by the platform layer (which owns audio, MIDI,
/// collaboration, and other platform-specific state) and passed to
/// [`IcedEmbed::new`](crate::IcedEmbed::new).
pub trait Program: 'static {
    /// The message type for this application.
    type Message: Clone + std::fmt::Debug;

    /// Handle a message (from UI interaction or tick).
    fn update(&mut self, message: Self::Message);

    /// Build the current view. Called once per frame after [`pre_frame`](Program::pre_frame)
    /// and the tick message.
    fn view(&self) -> Element<'_, Self::Message, Theme, Renderer>;

    /// Optional message to send at the start of each frame (before building the UI).
    ///
    /// For simple apps, return `Some(Message::Tick)` to poll external state
    /// each frame. For complex apps that handle multiple tick stages (e.g.
    /// MIDI input + sequencer + audio feedback), do all work in
    /// [`pre_frame`](Program::pre_frame) and return `None` here.
    fn tick_message() -> Option<Self::Message> {
        None
    }

    /// Notify the app that the viewport size changed (in logical points).
    fn set_viewport_size(&mut self, width: f32, height: f32);

    /// Serialize the app state to JSON for persistence.
    ///
    /// Called by the platform layer when the app enters the background or
    /// is about to be terminated.
    fn to_json(&self) -> String;

    /// Whether the app is currently animating and needs continuous redraw.
    ///
    /// Return `true` when there is active animation, audio visualization,
    /// or any other reason to keep rendering even if no UI events occurred.
    /// The default returns `false` (only redraw on input or iced request).
    fn is_animating(&self) -> bool {
        false
    }

    /// Called before each frame's UI build/update cycle.
    ///
    /// Use this to read external state (e.g. audio feedback from atomics)
    /// and prepare the app for rendering. This is called before the tick
    /// message is sent.
    fn pre_frame(&mut self) {}

    /// Called after all UI messages have been processed in a frame.
    ///
    /// Use this to flush state changes to external systems (e.g. push
    /// audio events to a ring buffer, sync collaboration state).
    fn post_update(&mut self) {}
}
