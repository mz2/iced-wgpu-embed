//! # iced-wgpu-embed
//!
//! Embed [iced](https://iced.rs) + [wgpu](https://wgpu.rs) views into native
//! iOS, Android, and desktop view hierarchies — without winit.
//!
//! ## Why
//!
//! [winit](https://github.com/rust-windowing/winit) doesn't support embedding
//! into an existing `UIViewController` or Android `Activity`. If you want to
//! render an iced UI inside a native app (alongside native navigation, audio
//! session management, etc.), you need to drive the wgpu surface and iced
//! rendering loop yourself. This crate provides that infrastructure.
//!
//! ## Core types
//!
//! - [`Program`] — Trait for your application (extends iced's Program concept
//!   with embedding lifecycle hooks).
//! - [`IcedEmbed`] — The rendering context: owns the wgpu surface, iced
//!   engine/renderer, and your `Program` instance.
//! - [`RedrawFlag`] / [`SimpleNotifier`] / [`CallbackNotifier`] — Redraw
//!   signaling between iced internals and your platform render loop.
//! - [`TouchPhase`] / [`TouchEvent`] — `#[repr(C)]` touch event types for FFI.
//! - [`fit_surface`] — Utility to downscale surfaces that exceed GPU texture limits.
//!
//! ## Quick start
//!
//! ```ignore
//! use iced_wgpu_embed::{EmbedConfig, IcedEmbed, Program, SimpleNotifier, RedrawFlag};
//!
//! // 1. Create wgpu instance + surface (platform-specific)
//! let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
//!     backends: wgpu::Backends::METAL, // or VULKAN on Android
//!     ..Default::default()
//! });
//! let surface = unsafe {
//!     instance.create_surface_unsafe(/* platform-specific target */)
//! }?;
//!
//! // 2. Create notifier + redraw flag
//! let redraw_flag = RedrawFlag::new();
//! let notifier = SimpleNotifier::new(&redraw_flag);
//!
//! // 3. Create the embedded iced rendering context
//! let config = EmbedConfig {
//!     instance, surface,
//!     width: 1920, height: 1080,
//!     scale_factor: 2.0,
//!     saved_state: None,
//!     extra_fonts: vec![],
//! };
//! let mut embed = IcedEmbed::<MyApp>::new(config, notifier, redraw_flag)?;
//!
//! // 4. Drive the render loop from your platform's vsync callback
//! let needs_redraw = embed.enter_frame();
//!
//! // 5. Forward touch events
//! embed.push_touch_event(TouchPhase::Started, finger_id, x, y);
//! ```
//!
//! ## Re-exports
//!
//! This crate re-exports [`wgpu`] and key iced types so consumers don't need
//! to coordinate dependency versions.

pub mod notifier;
pub mod program;
pub mod redraw_flag;
pub mod surface;
pub mod touch;
pub mod viewport;

// Re-export primary types at crate root
pub use notifier::{CallbackNotifier, SimpleNotifier};
pub use program::Program;
pub use redraw_flag::RedrawFlag;
pub use surface::{EmbedConfig, IcedEmbed};
pub use touch::{TouchEvent, TouchPhase};
pub use viewport::fit_surface;

// Re-export wgpu so consumers can create Instance/Surface without version conflicts
pub use wgpu;
