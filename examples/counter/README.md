# Counter Example

A minimal counter app built with `iced-wgpu-embed`, demonstrating iOS and Android integration.

## Structure

```
counter/
  Cargo.toml              # Rust library (staticlib + cdylib)
  src/lib.rs              # Counter Program impl + iOS FFI + Android JNI
  ios/CounterView.swift   # UIViewController with CAMetalLayer + CADisplayLink
  android/NativeBridge.kt # Activity with SurfaceView + Choreographer
```

## The Rust side

`src/lib.rs` contains:

1. **`Counter`** -- implements `iced_wgpu_embed::Program` with increment/decrement/reset buttons.
2. **iOS FFI** -- `extern "C"` functions (`counter_create`, `counter_enter_frame`, `counter_touch`, etc.) that Swift calls directly.
3. **Android JNI** -- `Java_com_example_counter_NativeBridge_*` functions that Kotlin calls via `System.loadLibrary`.

## Building for iOS

```bash
# Install the iOS target
rustup target add aarch64-apple-ios

# Build the static library
cargo build --target aarch64-apple-ios --release -p counter-example

# The static library is at:
# target/aarch64-apple-ios/release/libcounter_example.a
```

In Xcode:
1. Create a new iOS App project.
2. Add `libcounter_example.a` to the project (Link Binary With Libraries).
3. Add `ios/CounterView.swift` to the project.
4. Set `CounterViewController` as the root view controller (in `SceneDelegate` or storyboard).

## Building for Android

```bash
# Install cargo-ndk and the Android target
cargo install cargo-ndk
rustup target add aarch64-linux-android

# Build the shared library (requires Android NDK)
cargo ndk -t arm64-v8a build --release -p counter-example

# The shared library is at:
# target/aarch64-linux-android/release/libcounter_example.so
```

In Android Studio:
1. Create a new Android project.
2. Place `libcounter_example.so` in `app/src/main/jniLibs/arm64-v8a/`.
3. Add `android/NativeBridge.kt` to the project under `com.example.counter`.
4. Set `CounterActivity` as the launcher activity in `AndroidManifest.xml`.

## How it works

### Render loop

The native side (Swift/Kotlin) owns the render loop timing:
- **iOS**: `CADisplayLink` fires at the display refresh rate. Each tick calls `counter_enter_frame()`.
- **Android**: `Choreographer.doFrame()` fires each vsync. Each tick calls `NativeBridge.enterFrame()`.

### Touch events

- **iOS**: `UIViewController` touch methods (`touchesBegan`, etc.) forward events via `counter_touch()`. UIKit provides logical-point coordinates directly.
- **Android**: `Activity.onTouchEvent()` forwards events via `NativeBridge.touch()`. Android provides pixel coordinates, so the Kotlin side passes `displayScale` and the Rust side divides to get logical points.

### State persistence

Both platforms persist the counter value to platform storage (`UserDefaults` on iOS, `SharedPreferences` on Android) when the surface is destroyed, and restore it on creation.
