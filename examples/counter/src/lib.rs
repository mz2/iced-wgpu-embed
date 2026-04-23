//! Counter example: a minimal iced app embedded in a native iOS/Android view.
//!
//! This crate demonstrates how to:
//! 1. Implement the [`Program`] trait for a simple counter app.
//! 2. Export `extern "C"` functions that iOS (Swift) calls via FFI.
//! 3. Export JNI functions that Android (Kotlin) calls via `System.loadLibrary`.

use std::ffi::c_void;

use iced_core::{Alignment, Element, Length};
use iced_wgpu::Renderer;
use iced_widget::{button, column, container, row, text, Theme};

use iced_wgpu_embed::wgpu;
use iced_wgpu_embed::{
    CallbackNotifier, EmbedConfig, IcedEmbed, Program, RedrawFlag, TouchPhase,
};
#[cfg(target_os = "android")]
use iced_wgpu_embed::SimpleNotifier;

// ─── Program implementation ─────────────────────────────────────────────────

struct Counter {
    count: i32,
    viewport_width: f32,
    viewport_height: f32,
}

#[derive(Clone, Debug)]
enum Message {
    Tick,
    Increment,
    Decrement,
    Reset,
}

impl Counter {
    fn new(saved_state: Option<&str>) -> Self {
        let count = saved_state
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        Counter {
            count,
            viewport_width: 0.0,
            viewport_height: 0.0,
        }
    }
}

impl Program for Counter {
    type Message = Message;

    fn update(&mut self, message: Message) {
        match message {
            Message::Tick => {}
            Message::Increment => self.count += 1,
            Message::Decrement => self.count -= 1,
            Message::Reset => self.count = 0,
        }
    }

    fn view(&self) -> Element<'_, Message, Theme, Renderer> {
        let counter_text = text(format!("{}", self.count))
            .size(80);

        let controls = row![
            button(text("-").size(30).center())
                .on_press(Message::Decrement)
                .width(80)
                .height(60),
            button(text("Reset").size(20).center())
                .on_press(Message::Reset)
                .width(100)
                .height(60),
            button(text("+").size(30).center())
                .on_press(Message::Increment)
                .width(80)
                .height(60),
        ]
        .spacing(20)
        .align_y(Alignment::Center);

        container(
            column![counter_text, controls]
                .spacing(40)
                .align_x(Alignment::Center),
        )
        .center(Length::Fill)
        .into()
    }

    fn tick_message() -> Option<Message> {
        Some(Message::Tick)
    }

    fn set_viewport_size(&mut self, width: f32, height: f32) {
        self.viewport_width = width;
        self.viewport_height = height;
    }

    fn to_json(&self) -> String {
        self.count.to_string()
    }
}

// ─── iOS FFI ────────────────────────────────────────────────────────────────
//
// These extern "C" functions are called from Swift. The Swift side owns the
// CAMetalLayer, CADisplayLink (render loop), and UIGestureRecognizers (touch).

/// Callback type for Rust → Swift communication.
/// On iOS the native side passes a function pointer at creation time.
/// The only callback used in this example is RequestRedraw (to resume
/// CADisplayLink when iced requests a redraw while idle).
type SwiftCallback = extern "C" fn(i32, *const u8, usize);

const CALLBACK_REQUEST_REDRAW: i32 = 4;

/// Opaque handle stored on the Swift side as an UnsafeMutableRawPointer.
pub struct CounterHandle {
    embed: IcedEmbed<Counter>,
}

/// Create the iced rendering context from a CAMetalLayer.
///
/// # Safety
/// - `metal_layer` must be a valid `CAMetalLayer` pointer.
/// - `saved_state` must be valid UTF-8 of length `saved_len`, or null.
#[no_mangle]
pub unsafe extern "C" fn counter_create(
    metal_layer: *mut c_void,
    width: u32,
    height: u32,
    scale_factor: f32,
    callback: SwiftCallback,
    saved_state: *const u8,
    saved_len: usize,
) -> *mut CounterHandle {
    let state_json = if saved_state.is_null() || saved_len == 0 {
        None
    } else {
        let bytes = std::slice::from_raw_parts(saved_state, saved_len);
        std::str::from_utf8(bytes).ok().map(|s| s.to_string())
    };

    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::METAL,
        ..Default::default()
    });

    let surface = match instance.create_surface_unsafe(
        wgpu::SurfaceTargetUnsafe::CoreAnimationLayer(metal_layer),
    ) {
        Ok(s) => s,
        Err(e) => {
            log::error!("Failed to create surface: {e}");
            return std::ptr::null_mut();
        }
    };

    let redraw_flag = RedrawFlag::new();
    let notifier = CallbackNotifier::new(&redraw_flag, move || {
        (callback)(CALLBACK_REQUEST_REDRAW, std::ptr::null(), 0);
    });

    let counter = Counter::new(state_json.as_deref());

    let config = EmbedConfig {
        instance,
        surface,
        width,
        height,
        scale_factor,
        extra_fonts: vec![],
    };

    match IcedEmbed::new(config, counter, notifier, redraw_flag) {
        Ok(embed) => Box::into_raw(Box::new(CounterHandle { embed })),
        Err(e) => {
            log::error!("Failed to create IcedEmbed: {e}");
            std::ptr::null_mut()
        }
    }
}

/// Destroy the rendering context.
///
/// # Safety
/// `handle` must be a pointer from `counter_create`.
#[no_mangle]
pub unsafe extern "C" fn counter_destroy(handle: *mut CounterHandle) {
    if !handle.is_null() {
        drop(Box::from_raw(handle));
    }
}

/// Render one frame. Returns 1 if redraw needed, 0 if idle.
///
/// # Safety
/// `handle` must be a valid pointer from `counter_create`.
#[no_mangle]
pub unsafe extern "C" fn counter_enter_frame(handle: *mut CounterHandle) -> i32 {
    let handle = &mut *handle;
    handle.embed.enter_frame() as i32
}

/// Notify of a view resize.
///
/// # Safety
/// `handle` must be a valid pointer from `counter_create`.
#[no_mangle]
pub unsafe extern "C" fn counter_resize(
    handle: *mut CounterHandle,
    width: u32,
    height: u32,
    scale_factor: f32,
) {
    let handle = &mut *handle;
    handle.embed.resize(width, height, scale_factor);
}

/// Forward a touch event. Coordinates must be in logical points.
///
/// # Safety
/// `handle` must be a valid pointer from `counter_create`.
#[no_mangle]
pub unsafe extern "C" fn counter_touch(
    handle: *mut CounterHandle,
    phase: i32,
    finger_id: u64,
    x: f32,
    y: f32,
) {
    let handle = &mut *handle;
    let phase = match phase {
        0 => TouchPhase::Started,
        1 => TouchPhase::Moved,
        2 => TouchPhase::Ended,
        _ => TouchPhase::Cancelled,
    };
    handle.embed.push_touch_event(phase, finger_id, x, y);
}

/// Get the serialized state for persistence.
///
/// Returns a pointer to a null-terminated UTF-8 string. The caller must
/// free it with `counter_free_string`.
///
/// # Safety
/// `handle` must be a valid pointer from `counter_create`.
#[no_mangle]
pub unsafe extern "C" fn counter_get_state(
    handle: *const CounterHandle,
) -> *mut std::os::raw::c_char {
    let handle = &*handle;
    let json = handle.embed.program().to_json();
    match std::ffi::CString::new(json) {
        Ok(cstr) => cstr.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Free a string returned by `counter_get_state`.
///
/// # Safety
/// `ptr` must be a pointer from `counter_get_state`, or null.
#[no_mangle]
pub unsafe extern "C" fn counter_free_string(ptr: *mut std::os::raw::c_char) {
    if !ptr.is_null() {
        drop(std::ffi::CString::from_raw(ptr));
    }
}

// ─── Android JNI ────────────────────────────────────────────────────────────
//
// These functions are called from Kotlin via JNI. The Kotlin side owns the
// SurfaceView, Choreographer (render loop), and touch events.

#[cfg(target_os = "android")]
mod android {
    use super::*;
    use jni::objects::{JClass, JObject, JString};
    use jni::sys::{jfloat, jint, jlong};
    use jni::JNIEnv;

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_com_example_counter_NativeBridge_create(
        mut env: JNIEnv,
        _class: JClass,
        surface: JObject,
        width: jint,
        height: jint,
        scale_factor: jfloat,
        saved_state: JString,
    ) -> jlong {
        android_logger::init_once(
            android_logger::Config::default()
                .with_max_level(log::LevelFilter::Info)
                .with_tag("counter"),
        );

        let state_json: Option<String> = if saved_state.is_null() {
            None
        } else {
            env.get_string(&saved_state).ok().map(|s| s.into())
        };

        // Get ANativeWindow from the Surface
        let native_window = unsafe {
            let raw = ndk_sys::ANativeWindow_fromSurface(
                env.get_raw() as *mut _,
                surface.as_raw() as *mut _,
            );
            let Some(non_null) = std::ptr::NonNull::new(raw) else {
                log::error!("ANativeWindow_fromSurface returned null");
                return 0;
            };
            ndk::native_window::NativeWindow::from_ptr(non_null)
        };

        // Create wgpu surface from ANativeWindow
        use raw_window_handle::{
            AndroidNdkWindowHandle, HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
            RawWindowHandle,
        };

        struct WindowHandle(ndk::native_window::NativeWindow);

        impl HasWindowHandle for WindowHandle {
            fn window_handle(
                &self,
            ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError>
            {
                let ptr = self.0.ptr().as_ptr() as *mut c_void;
                let non_null = std::ptr::NonNull::new(ptr).unwrap();
                let handle = AndroidNdkWindowHandle::new(non_null);
                let raw = RawWindowHandle::AndroidNdk(handle);
                Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(raw) })
            }
        }

        impl HasDisplayHandle for WindowHandle {
            fn display_handle(
                &self,
            ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError>
            {
                let raw =
                    RawDisplayHandle::Android(raw_window_handle::AndroidDisplayHandle::new());
                Ok(unsafe { raw_window_handle::DisplayHandle::borrow_raw(raw) })
            }
        }

        let wh = WindowHandle(native_window);

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..Default::default()
        });

        let surface = unsafe {
            let raw_window = wh.window_handle().unwrap();
            let raw_display = wh.display_handle().unwrap();
            match instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: raw_display.into(),
                raw_window_handle: raw_window.into(),
            }) {
                Ok(s) => s,
                Err(e) => {
                    log::error!("Failed to create surface: {e}");
                    return 0;
                }
            }
        };

        let counter = Counter::new(state_json.as_deref());

        let redraw_flag = RedrawFlag::new();
        let notifier = SimpleNotifier::new(&redraw_flag);

        let config = EmbedConfig {
            instance,
            surface,
            width: width as u32,
            height: height as u32,
            scale_factor,
            extra_fonts: vec![],
        };

        match IcedEmbed::new(config, counter, notifier, redraw_flag) {
            Ok(embed) => {
                let handle = Box::new(CounterHandle { embed });
                Box::into_raw(handle) as jlong
            }
            Err(e) => {
                log::error!("Failed to create IcedEmbed: {e}");
                0
            }
        }
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_com_example_counter_NativeBridge_destroy(
        _env: JNIEnv,
        _class: JClass,
        handle: jlong,
    ) {
        if handle != 0 {
            unsafe { drop(Box::from_raw(handle as *mut CounterHandle)) };
        }
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_com_example_counter_NativeBridge_enterFrame(
        _env: JNIEnv,
        _class: JClass,
        handle: jlong,
    ) -> jint {
        let handle = unsafe { &mut *(handle as *mut CounterHandle) };
        handle.embed.enter_frame() as jint
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_com_example_counter_NativeBridge_resize(
        _env: JNIEnv,
        _class: JClass,
        handle: jlong,
        width: jint,
        height: jint,
        scale_factor: jfloat,
    ) {
        let handle = unsafe { &mut *(handle as *mut CounterHandle) };
        handle
            .embed
            .resize(width as u32, height as u32, scale_factor);
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_com_example_counter_NativeBridge_touch(
        _env: JNIEnv,
        _class: JClass,
        handle: jlong,
        action: jint,
        finger_id: jint,
        x: jfloat,
        y: jfloat,
        display_scale: jfloat,
    ) {
        let handle = unsafe { &mut *(handle as *mut CounterHandle) };
        let phase = match action {
            0 => TouchPhase::Started,
            1 => TouchPhase::Ended,
            2 => TouchPhase::Moved,
            _ => TouchPhase::Cancelled,
        };
        // Android MotionEvent coordinates are in physical pixels — convert to logical
        handle.embed.push_touch_event(
            phase,
            finger_id as u64,
            x / display_scale,
            y / display_scale,
        );
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_com_example_counter_NativeBridge_getState<'local>(
        env: JNIEnv<'local>,
        _class: JClass,
        handle: jlong,
    ) -> jni::objects::JObject<'local> {
        let handle = unsafe { &*(handle as *const CounterHandle) };
        let json = handle.embed.program().to_json();
        env.new_string(&json)
            .map(|s| s.into())
            .unwrap_or_else(|_| jni::objects::JObject::null())
    }
}
