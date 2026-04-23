// CounterView.swift
//
// Minimal iOS integration for the iced-wgpu-embed counter example.
//
// This UIViewController hosts a CAMetalLayer and drives the iced render
// loop via CADisplayLink. Touch events are forwarded to the Rust side.
//
// Add this file to an Xcode project that links the counter-example
// static library (libcounter_example.a) built with:
//   cargo build --target aarch64-apple-ios --release -p counter-example

import UIKit
import QuartzCore

// MARK: - FFI declarations

// These match the extern "C" functions in examples/counter/src/lib.rs

typealias SwiftCallback = @convention(c) (Int32, UnsafePointer<UInt8>?, Int) -> Void

@_silgen_name("counter_create")
func counter_create(
    _ metalLayer: UnsafeMutableRawPointer,
    _ width: UInt32,
    _ height: UInt32,
    _ scaleFactor: Float,
    _ callback: SwiftCallback,
    _ savedState: UnsafePointer<UInt8>?,
    _ savedLen: Int
) -> UnsafeMutableRawPointer?

@_silgen_name("counter_destroy")
func counter_destroy(_ handle: UnsafeMutableRawPointer)

@_silgen_name("counter_enter_frame")
func counter_enter_frame(_ handle: UnsafeMutableRawPointer) -> Int32

@_silgen_name("counter_resize")
func counter_resize(
    _ handle: UnsafeMutableRawPointer,
    _ width: UInt32,
    _ height: UInt32,
    _ scaleFactor: Float
)

@_silgen_name("counter_touch")
func counter_touch(
    _ handle: UnsafeMutableRawPointer,
    _ phase: Int32,
    _ fingerId: UInt64,
    _ x: Float,
    _ y: Float
)

@_silgen_name("counter_get_state")
func counter_get_state(_ handle: UnsafeRawPointer) -> UnsafeMutablePointer<CChar>?

@_silgen_name("counter_free_string")
func counter_free_string(_ ptr: UnsafeMutablePointer<CChar>?)

// MARK: - View Controller

class CounterViewController: UIViewController {
    private var metalLayer: CAMetalLayer!
    private var displayLink: CADisplayLink?
    private var handle: UnsafeMutableRawPointer?

    override func loadView() {
        let v = UIView()
        v.backgroundColor = .black
        metalLayer = CAMetalLayer()
        metalLayer.frame = v.bounds
        metalLayer.contentsScale = UIScreen.main.scale
        v.layer.addSublayer(metalLayer)
        self.view = v
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)

        metalLayer.frame = view.bounds
        let scale = Float(view.contentScaleFactor)
        let w = UInt32(view.bounds.width * CGFloat(scale))
        let h = UInt32(view.bounds.height * CGFloat(scale))

        // Restore saved state from UserDefaults
        let saved = UserDefaults.standard.string(forKey: "counter_state")

        // The callback lets Rust signal that iced needs a redraw
        // (e.g. after an animation or layout invalidation).
        let callback: SwiftCallback = { type_, _, _ in
            if type_ == 4 { // RequestRedraw
                // CADisplayLink will fire on the next vsync
                // (it's already running — nothing to do here in this simple example)
            }
        }

        let statePtr = saved?.utf8CString.withUnsafeBufferPointer { buf in
            return buf.baseAddress.map { ($0, buf.count - 1) } // exclude null terminator
        }

        handle = counter_create(
            Unmanaged.passUnretained(metalLayer).toOpaque(),
            w, h, scale,
            callback,
            statePtr.map { UnsafeRawPointer($0.0).assumingMemoryBound(to: UInt8.self) },
            statePtr?.1 ?? 0
        )

        guard handle != nil else {
            print("counter_create failed")
            return
        }

        // Start the render loop
        displayLink = CADisplayLink(target: self, selector: #selector(renderFrame))
        displayLink?.add(to: .main, forMode: .common)
    }

    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)
        displayLink?.invalidate()
        displayLink = nil

        // Persist state
        if let h = handle, let cstr = counter_get_state(h) {
            UserDefaults.standard.set(String(cString: cstr), forKey: "counter_state")
            counter_free_string(cstr)
        }

        if let h = handle {
            counter_destroy(h)
            handle = nil
        }
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        metalLayer.frame = view.bounds

        guard let h = handle else { return }
        let scale = Float(view.contentScaleFactor)
        let w = UInt32(view.bounds.width * CGFloat(scale))
        let height = UInt32(view.bounds.height * CGFloat(scale))
        counter_resize(h, w, height, scale)
    }

    @objc private func renderFrame() {
        guard let h = handle else { return }
        let needsRedraw = counter_enter_frame(h)
        // Optionally pause CADisplayLink when idle:
        // displayLink?.isPaused = (needsRedraw == 0)
        _ = needsRedraw
    }

    // MARK: - Touch forwarding

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        forwardTouches(touches, phase: 0) // Started
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
        forwardTouches(touches, phase: 1) // Moved
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        forwardTouches(touches, phase: 2) // Ended
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
        forwardTouches(touches, phase: 3) // Cancelled
    }

    private func forwardTouches(_ touches: Set<UITouch>, phase: Int32) {
        guard let h = handle else { return }
        for touch in touches {
            // UIKit touch coordinates are already in logical points
            let loc = touch.location(in: view)
            let fingerId = UInt64(touch.hash)
            counter_touch(h, phase, fingerId, Float(loc.x), Float(loc.y))
        }
    }
}
