//! The [`IcedEmbed`] struct: wgpu surface management and iced render loop.
//!
//! `IcedEmbed<P>` owns the full wgpu + iced rendering context for an
//! embedded application. It is generic over a [`Program`] implementation
//! that provides the application logic.
//!
//! The platform layer creates a `wgpu::Surface` (from a `CAMetalLayer` on
//! iOS/macOS, or an `ANativeWindow` on Android) and passes it to
//! [`IcedEmbed::new`]. From there, the platform drives the render loop by
//! calling [`enter_frame`](IcedEmbed::enter_frame) on each vsync.

use std::sync::Arc;
use std::time::Instant;

use iced_core::mouse;
use iced_core::renderer::Style as RendererStyle;
use iced_core::window;
use iced_core::{Color, Font, Pixels, Size};
use iced_runtime::user_interface::{self, UserInterface};
use iced_wgpu::graphics::shell::Shell;
use iced_wgpu::graphics::Viewport;
use iced_wgpu::{wgpu, Engine, Renderer};

use crate::diagnostics::Diagnostics;
use crate::touch::{translate_touch_events, TouchEvent, TouchPhase};
use crate::viewport::fit_surface;
use crate::Program;
use crate::RedrawFlag;

/// Configuration for creating an [`IcedEmbed`] instance.
pub struct EmbedConfig {
    /// A `wgpu::Instance` configured for the correct backend
    /// (e.g. `Backends::METAL` on iOS, `Backends::VULKAN` on Android).
    pub instance: wgpu::Instance,
    /// A `wgpu::Surface` created from the platform's native layer
    /// (e.g. `CAMetalLayer`, `ANativeWindow`).
    pub surface: wgpu::Surface<'static>,
    /// Physical surface width in pixels.
    pub width: u32,
    /// Physical surface height in pixels.
    pub height: u32,
    /// Display scale factor (e.g. 2.0 for Retina, 3.0 for xxxhdpi).
    pub scale_factor: f32,
    /// Optional JSON string to restore the program's state.
    pub saved_state: Option<String>,
    /// Additional font data to load (e.g. bold weights).
    /// Each slice must be `'static` (e.g. from `include_bytes!`).
    /// Fira Sans Regular is always loaded automatically.
    pub extra_fonts: Vec<&'static [u8]>,
}

/// The iced rendering context, embedded into a native view hierarchy.
///
/// `IcedEmbed<P>` manages all wgpu and iced state for an embedded application.
/// The platform layer owns the render loop timing (e.g. `CADisplayLink` on iOS,
/// `Choreographer` on Android) and calls [`enter_frame`](IcedEmbed::enter_frame)
/// each tick.
///
/// # Type Parameter
///
/// - `P`: The [`Program`] implementation that provides application logic.
///
/// # Lifecycle
///
/// 1. Platform creates a `wgpu::Instance` and `wgpu::Surface` (platform-specific).
/// 2. Call [`IcedEmbed::new`] with the instance, surface, dimensions, and a notifier.
/// 3. On each vsync: call [`enter_frame`](IcedEmbed::enter_frame). Returns `true`
///    if another frame is needed.
/// 4. On resize: call [`resize`](IcedEmbed::resize).
/// 5. Forward touch/cursor events via [`push_touch_event`](IcedEmbed::push_touch_event)
///    or [`push_touch_events`](IcedEmbed::push_touch_events).
/// 6. On background: call [`program().to_json()`](Program::to_json) to persist state.
pub struct IcedEmbed<P: Program> {
    // wgpu state
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    format: wgpu::TextureFormat,
    max_texture_dimension: u32,

    // iced rendering
    renderer: Renderer,
    viewport: Viewport,
    cache: user_interface::Cache,
    cursor: mouse::Cursor,
    /// Clear cursor to Unavailable after the current frame completes.
    /// This prevents hover highlights from staying stuck after a touch lift
    /// on touch-only platforms.
    clear_cursor_after_frame: bool,
    pending_events: Vec<iced_core::Event>,

    // Application
    program: P,
    redraw_flag: RedrawFlag,
    theme: iced_widget::Theme,

    // Diagnostics
    diagnostics: Arc<Diagnostics>,
}

impl<P: Program> IcedEmbed<P> {
    /// Create a new embedded iced rendering context.
    ///
    /// See [`EmbedConfig`] for the configuration parameters.
    ///
    /// # Errors
    ///
    /// Returns an error string if wgpu adapter/device creation or surface
    /// configuration fails.
    pub fn new(
        config: EmbedConfig,
        notifier: impl iced_wgpu::graphics::shell::Notifier + 'static,
        redraw_flag: RedrawFlag,
    ) -> Result<Self, String> {
        let EmbedConfig {
            instance,
            surface,
            width,
            height,
            scale_factor,
            saved_state,
            extra_fonts,
        } = config;

        log::info!(
            "IcedEmbed::new: {}x{} @ {:.1}x",
            width,
            height,
            scale_factor,
        );

        let diagnostics = Arc::new(Diagnostics::new());

        // ── wgpu adapter + device setup ──
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .map_err(|e| format!("Failed to find a suitable GPU adapter: {e}"))?;

        log::info!("GPU adapter: {:?}", adapter.get_info().name);

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("iced-wgpu-embed"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
            },
        ))
        .map_err(|e| format!("Failed to create GPU device: {e}"))?;

        // ── Surface configuration ──
        let surface_caps = surface.get_capabilities(&adapter);
        if surface_caps.formats.is_empty() {
            return Err(format!(
                "Surface has no supported formats (adapter: {:?}). \
                 This usually means the GPU/driver doesn't support the surface.",
                adapter.get_info().name,
            ));
        }
        let format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let max_texture_dimension = device.limits().max_texture_dimension_2d;
        let (surface_width, surface_height, effective_scale) =
            fit_surface(width, height, scale_factor, max_texture_dimension);

        log::info!(
            "Surface: {}x{} @ {:.2}x (requested {}x{} @ {:.1}x, max texture {})",
            surface_width,
            surface_height,
            effective_scale,
            width,
            height,
            scale_factor,
            max_texture_dimension,
        );

        surface.configure(
            &device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width: surface_width,
                height: surface_height,
                present_mode: wgpu::PresentMode::AutoVsync,
                alpha_mode: surface_caps
                    .alpha_modes
                    .first()
                    .copied()
                    .unwrap_or(wgpu::CompositeAlphaMode::Auto),
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            },
        );

        // ── iced rendering setup ──
        let shell = Shell::new(notifier);

        let engine = Engine::new(
            &adapter,
            device.clone(),
            queue.clone(),
            format,
            None::<iced_wgpu::graphics::Antialiasing>,
            shell,
        );

        // Load Fira Sans Regular (embedded via iced_graphics fira-sans feature).
        // Platforms without system font paths (iOS, Android) need this for
        // cosmic-text to find any usable font.
        {
            let mut fs = iced_wgpu::graphics::text::font_system()
                .write()
                .expect("Write to font system");
            fs.load_font(std::borrow::Cow::Borrowed(
                iced_wgpu::graphics::text::FIRA_SANS_REGULAR,
            ));
            for font_data in &extra_fonts {
                fs.load_font(std::borrow::Cow::Borrowed(font_data));
            }
        }

        let default_font = Font {
            family: iced_core::font::Family::Name("Fira Sans"),
            ..Font::default()
        };
        let renderer = Renderer::new(
            engine,
            iced_core::renderer::Settings {
                default_font,
                default_text_size: Pixels::from(16),
            },
        );

        let viewport = Viewport::with_physical_size(
            Size::new(surface_width, surface_height),
            effective_scale,
        );

        let cache = user_interface::Cache::new();

        // ── Program state ──
        let mut program = P::new(saved_state.as_deref());
        let logical = viewport.logical_size();
        program.set_viewport_size(logical.width, logical.height);

        log::info!("IcedEmbed initialized successfully");

        Ok(Self {
            surface,
            device,
            format,
            max_texture_dimension,
            renderer,
            viewport,
            cache,
            cursor: mouse::Cursor::Unavailable,
            clear_cursor_after_frame: false,
            pending_events: Vec::new(),
            program,
            redraw_flag,
            theme: iced_widget::Theme::Dark,
            diagnostics,
        })
    }

    /// Render one frame. Returns `true` if another redraw is needed.
    ///
    /// Call this from the platform's vsync callback (e.g. `CADisplayLink`
    /// on iOS, `Choreographer.doFrame` on Android).
    ///
    /// The render loop:
    /// 1. Acquires a surface texture from wgpu.
    /// 2. Calls [`Program::pre_frame`] and sends [`Program::tick_message`].
    /// 3. Builds the iced `UserInterface`, processes pending events.
    /// 4. Draws the UI and presents the frame.
    /// 5. Calls [`Program::post_update`] for post-frame work.
    /// 6. Returns whether another frame is needed.
    pub fn enter_frame(&mut self) -> bool {
        let frame_start = Instant::now();

        // ── Acquire surface texture ──
        let output = match self.surface.get_current_texture() {
            Ok(output) => output,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                log::warn!("Surface lost/outdated, will reconfigure");
                return true;
            }
            Err(e) => {
                log::error!("Surface error: {e}");
                return false;
            }
        };

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // ── Pre-frame hook + tick ──
        self.program.pre_frame();
        self.program.update(P::tick_message());

        // ── Build and update iced UI ──
        let mut events = std::mem::take(&mut self.pending_events);
        events.push(iced_core::Event::Window(
            iced_core::window::Event::RedrawRequested(Instant::now()),
        ));

        let mut interface = UserInterface::build(
            self.program.view(),
            self.viewport.logical_size(),
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );

        let mut messages = Vec::new();
        let (ui_state, _statuses) =
            interface.update(&events, self.cursor, &mut self.renderer, &mut messages);

        // Draw (before processing messages — messages apply on next frame)
        interface.draw(
            &mut self.renderer,
            &self.theme,
            &RendererStyle {
                text_color: Color::WHITE,
            },
            self.cursor,
        );

        self.cache = interface.into_cache();

        // ── Process messages ──
        let has_messages = !messages.is_empty();
        for msg in messages {
            self.program.update(msg);
        }

        // Post-update hook: flush audio events, collab, etc.
        self.program.post_update();

        // ── Present ──
        self.renderer
            .present(None, self.format, &view, &self.viewport);

        output.present();

        // ── Diagnostics ──
        let elapsed = frame_start.elapsed();
        self.diagnostics
            .record_frame_time_us(elapsed.as_micros() as u64);

        // ── Redraw determination ──
        let iced_wants_redraw = self.redraw_flag.take();
        let ui_needs_redraw = matches!(
            ui_state,
            user_interface::State::Outdated
                | user_interface::State::Updated {
                    redraw_request: window::RedrawRequest::NextFrame,
                    ..
                }
        );
        let app_animating = self.program.is_animating();

        // Clear cursor after frame if last touch was a lift.
        // This prevents hover highlights from staying stuck on touch platforms.
        if self.clear_cursor_after_frame {
            self.cursor = mouse::Cursor::Unavailable;
            self.clear_cursor_after_frame = false;
        }

        has_messages || ui_needs_redraw || iced_wants_redraw || app_animating
    }

    /// Handle a view resize.
    ///
    /// Call this when the native view's size changes (e.g. rotation,
    /// multi-window, split-screen).
    ///
    /// # Arguments
    ///
    /// - `width`, `height` — New physical surface dimensions in pixels.
    /// - `scale_factor` — New display scale factor.
    pub fn resize(&mut self, width: u32, height: u32, scale_factor: f32) {
        let (w, h, effective_scale) =
            fit_surface(width, height, scale_factor, self.max_texture_dimension);

        log::info!("resize: {}x{} @ {:.2}x", w, h, effective_scale);

        self.viewport = Viewport::with_physical_size(Size::new(w, h), effective_scale);
        let logical = self.viewport.logical_size();
        self.program.set_viewport_size(logical.width, logical.height);

        // Invalidate the iced layout cache so the UI rebuilds for the new size.
        self.cache = user_interface::Cache::new();

        self.surface.configure(
            &self.device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: self.format,
                width: w,
                height: h,
                present_mode: wgpu::PresentMode::AutoVsync,
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            },
        );
    }

    /// Queue a single touch event for the next frame.
    ///
    /// Coordinates must be in **logical points** (not physical pixels).
    /// On Android, divide `MotionEvent.getX()` by the display density
    /// before calling this.
    ///
    /// Cursor tracking is updated automatically:
    /// - Press/Move: cursor becomes `Available` at the touch position.
    /// - Lift: cursor stays `Available` for the current frame (so the lift
    ///   event is processed with a position), then clears after the frame.
    /// - Cancel: cursor immediately becomes `Unavailable`.
    pub fn push_touch_event(&mut self, phase: TouchPhase, finger_id: u64, x: f32, y: f32) {
        let id = iced_core::touch::Finger(finger_id);
        let position = iced_core::Point::new(x, y);

        let touch_event = match phase {
            TouchPhase::Started => iced_core::touch::Event::FingerPressed { id, position },
            TouchPhase::Moved => iced_core::touch::Event::FingerMoved { id, position },
            TouchPhase::Ended => iced_core::touch::Event::FingerLifted { id, position },
            TouchPhase::Cancelled => iced_core::touch::Event::FingerLost { id, position },
        };

        // Update cursor tracking
        match phase {
            TouchPhase::Started | TouchPhase::Moved => {
                self.cursor = mouse::Cursor::Available(position);
                self.clear_cursor_after_frame = false;
            }
            TouchPhase::Ended => {
                self.cursor = mouse::Cursor::Available(position);
                self.clear_cursor_after_frame = true;
            }
            TouchPhase::Cancelled => {
                self.cursor = mouse::Cursor::Unavailable;
            }
        }

        // Emit CursorMoved before FingerPressed so iced's cursor position
        // is correct when widgets check cursor.is_over(bounds).
        if matches!(phase, TouchPhase::Started) {
            self.pending_events.push(iced_core::Event::Mouse(
                iced_core::mouse::Event::CursorMoved { position },
            ));
        }

        self.pending_events
            .push(iced_core::Event::Touch(touch_event));
    }

    /// Queue a batch of touch events for the next frame.
    ///
    /// Coordinates must be in **logical points**. Cursor tracking is
    /// updated based on the touch phases (see [`push_touch_event`](Self::push_touch_event)).
    pub fn push_touch_events(&mut self, events: &[TouchEvent]) {
        let iced_events = translate_touch_events(events);

        // Track cursor position from touch phases
        for event in &iced_events {
            if let iced_core::Event::Touch(touch_event) = event {
                match touch_event {
                    iced_core::touch::Event::FingerPressed { position, .. }
                    | iced_core::touch::Event::FingerMoved { position, .. } => {
                        self.cursor = mouse::Cursor::Available(*position);
                        self.clear_cursor_after_frame = false;
                    }
                    iced_core::touch::Event::FingerLifted { position, .. }
                    | iced_core::touch::Event::FingerLost { position, .. } => {
                        self.cursor = mouse::Cursor::Available(*position);
                        self.clear_cursor_after_frame = true;
                    }
                }
            }
        }

        self.pending_events.extend(iced_events);
    }

    /// Queue a cursor-moved (hover) event.
    ///
    /// Used on platforms with mouse/trackpad support (e.g. macOS via
    /// Mac Catalyst's `UIHoverGestureRecognizer`).
    pub fn push_cursor_moved(&mut self, x: f32, y: f32) {
        let position = iced_core::Point::new(x, y);
        self.cursor = mouse::Cursor::Available(position);
        self.clear_cursor_after_frame = false;
        self.pending_events.push(iced_core::Event::Mouse(
            iced_core::mouse::Event::CursorMoved { position },
        ));
    }

    /// Notify that the cursor has left the view.
    pub fn push_cursor_exited(&mut self) {
        self.cursor = mouse::Cursor::Unavailable;
        self.pending_events.push(iced_core::Event::Mouse(
            iced_core::mouse::Event::CursorLeft,
        ));
    }

    /// Tick the program without rendering.
    ///
    /// Use this for background processing when the app is not visible
    /// but still needs to drive state (e.g. a playing sequencer).
    pub fn background_tick(&mut self) {
        self.program.pre_frame();
        self.program.update(P::tick_message());
        self.program.post_update();
    }

    /// Immutable access to the embedded program.
    pub fn program(&self) -> &P {
        &self.program
    }

    /// Mutable access to the embedded program.
    pub fn program_mut(&mut self) -> &mut P {
        &mut self.program
    }

    /// Access the shared diagnostics counters.
    pub fn diagnostics(&self) -> &Arc<Diagnostics> {
        &self.diagnostics
    }

    /// Access the current viewport.
    pub fn viewport(&self) -> &Viewport {
        &self.viewport
    }

    /// The wgpu texture format used for the surface.
    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.format
    }
}
