# iced-wgpu-embed

Embed [iced](https://iced.rs) + [wgpu](https://wgpu.rs) views into native iOS, Android, and desktop view hierarchies -- without winit.

## Why

[winit](https://github.com/rust-windowing/winit) doesn't support embedding into an existing `UIViewController` or Android `Activity`. If you want to render an iced UI inside a native app -- alongside native navigation, audio session management, platform-specific features -- you need to drive the wgpu surface and iced rendering loop yourself.

This crate provides that infrastructure: wgpu setup (adapter, device, surface configuration), iced engine/renderer initialization, the per-frame render loop, touch event translation, viewport management, and redraw signaling. You implement a `Program` trait with your application logic; `IcedEmbed` handles everything else.

## Overview

```
Native App (Swift / Kotlin / C++)
  |
  |-- Creates CAMetalLayer (iOS) or ANativeWindow (Android)
  |-- Creates wgpu::Instance + wgpu::Surface
  |-- Creates IcedEmbed<MyApp>
  |
  |-- On vsync: embed.enter_frame() -> bool (needs another frame?)
  |-- On touch: embed.push_touch_event(phase, finger, x, y)
  |-- On resize: embed.resize(w, h, scale)
  |-- On background: embed.program().to_json() -> persist
```

## Usage

### 1. Implement the `Program` trait

```rust
use iced_wgpu_embed::Program;
use iced_core::Element;
use iced_widget::{button, column, text, Theme};
use iced_wgpu::Renderer;

struct Counter {
    count: i32,
}

#[derive(Clone, Debug)]
enum Message {
    Tick,
    Increment,
    Decrement,
}

impl Program for Counter {
    type Message = Message;

    fn new(saved_state: Option<&str>) -> Self {
        let count = saved_state
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        Counter { count }
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::Tick => {}
            Message::Increment => self.count += 1,
            Message::Decrement => self.count -= 1,
        }
    }

    fn view(&self) -> Element<'_, Message, Theme, Renderer> {
        column![
            button("+").on_press(Message::Increment),
            text(self.count),
            button("-").on_press(Message::Decrement),
        ]
        .into()
    }

    fn tick_message() -> Message {
        Message::Tick
    }

    fn set_viewport_size(&mut self, _width: f32, _height: f32) {}

    fn to_json(&self) -> String {
        self.count.to_string()
    }
}
```

### 2. Create the wgpu surface (platform-specific)

#### iOS (Metal)

```rust
use iced_wgpu_embed::wgpu;

let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
    backends: wgpu::Backends::METAL,
    ..Default::default()
});

// metal_layer: *mut c_void from CAMetalLayer
let surface = unsafe {
    instance.create_surface_unsafe(
        wgpu::SurfaceTargetUnsafe::CoreAnimationLayer(metal_layer)
    ).expect("Failed to create surface")
};
```

#### Android (Vulkan)

```rust
use iced_wgpu_embed::wgpu;

let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
    backends: wgpu::Backends::VULKAN,
    ..Default::default()
});

// Create surface from ANativeWindow via raw-window-handle
let surface = unsafe {
    instance.create_surface_unsafe(
        wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: display_handle.into(),
            raw_window_handle: window_handle.into(),
        }
    ).expect("Failed to create surface")
};
```

### 3. Create `IcedEmbed` and drive the render loop

```rust
use iced_wgpu_embed::{EmbedConfig, IcedEmbed, SimpleNotifier, RedrawFlag};

let redraw_flag = RedrawFlag::new();
let notifier = SimpleNotifier::new(&redraw_flag);

let config = EmbedConfig {
    instance,
    surface,
    width: 1920,
    height: 1080,
    scale_factor: 2.0,
    saved_state: None,
    extra_fonts: vec![],
};

let mut embed = IcedEmbed::<Counter>::new(config, notifier, redraw_flag)
    .expect("Failed to create IcedEmbed");

// In your vsync callback (CADisplayLink / Choreographer):
let needs_redraw = embed.enter_frame();

// Forward touch events (coordinates in logical points):
embed.push_touch_event(TouchPhase::Started, finger_id, x, y);

// On resize:
embed.resize(new_width, new_height, new_scale);

// Persist state on background:
let json = embed.program().to_json();
```

## Notifiers

The platform's render loop needs to know when iced requests a redraw. Two notifier implementations are provided:

- **`SimpleNotifier`** -- Sets an atomic flag. Use on platforms where the render loop polls continuously (e.g. Android Choreographer).

- **`CallbackNotifier`** -- Sets the flag and calls a closure. Use on platforms that pause the render loop when idle (e.g. iOS CADisplayLink):

```rust
use iced_wgpu_embed::{CallbackNotifier, RedrawFlag};

let redraw_flag = RedrawFlag::new();
let notifier = CallbackNotifier::new(&redraw_flag, || {
    // Resume CADisplayLink or signal the render thread
});
```

## Touch handling

Touch events are translated to iced touch/cursor events automatically. Coordinates must be in **logical points** (not physical pixels). On Android, divide `MotionEvent.getX()` by the display density before calling `push_touch_event`.

Two APIs are available:
- `push_touch_event(phase, finger_id, x, y)` -- single event (Android style)
- `push_touch_events(&[TouchEvent])` -- batch of `#[repr(C)]` events (iOS style, for C FFI)

Cursor tracking handles touch-specific UX: the cursor stays available during a finger lift (so iced processes the release at the correct position) then clears after the frame to avoid stuck hover highlights.

## Lifecycle hooks

The `Program` trait includes optional lifecycle hooks:

- **`pre_frame()`** -- Called before each frame's tick message. Use this to read external state (e.g. audio feedback from atomics, network updates).

- **`post_update()`** -- Called after all UI messages have been processed. Use this to flush state changes to external systems (e.g. push audio events to a ring buffer).

- **`is_animating()`** -- Return `true` when the app needs continuous redraw (e.g. active audio visualization, playing animations).

- **`background_tick()`** -- Available on `IcedEmbed` to tick the program without rendering, for background processing.

## Viewport scaling

High-DPI displays can produce surface dimensions that exceed GPU texture limits. The `fit_surface()` utility (used internally by `IcedEmbed`) reduces the effective scale factor to keep the logical size correct while respecting hardware limits. It is also exported for direct use:

```rust
use iced_wgpu_embed::fit_surface;

let (width, height, effective_scale) = fit_surface(
    12288, 6144, 3.0, // physical dims + scale
    8192,              // GPU max texture dimension
);
assert!(width <= 8192);
```

## API reference

| Type | Description |
|------|-------------|
| `Program` | Trait for your application (iced Program + embedding lifecycle) |
| `IcedEmbed<P>` | The rendering context: wgpu surface + iced engine + your Program |
| `EmbedConfig` | Configuration struct for `IcedEmbed::new()` |
| `RedrawFlag` | Shared atomic flag for redraw signaling |
| `SimpleNotifier` | Flag-only notifier (Android style) |
| `CallbackNotifier<F>` | Flag + closure notifier (iOS style) |
| `TouchPhase` | `#[repr(C)]` touch phase enum |
| `TouchEvent` | `#[repr(C)]` touch event struct for FFI batching |
| `Diagnostics` | Atomic frame-time and underrun counters |
| `fit_surface()` | Viewport scaling for GPU texture limits |

## License

Apache 2.0
