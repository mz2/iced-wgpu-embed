// NativeBridge.kt
//
// Minimal Android integration for the iced-wgpu-embed counter example.
//
// This Activity hosts a SurfaceView and drives the iced render loop via
// Choreographer. Touch events are forwarded to the Rust side.
//
// Add this file to an Android project that loads the counter-example
// shared library built with:
//   cargo ndk -t arm64-v8a build --release -p counter-example

package com.example.counter

import android.app.Activity
import android.os.Bundle
import android.view.Choreographer
import android.view.MotionEvent
import android.view.Surface
import android.view.SurfaceHolder
import android.view.SurfaceView

class CounterActivity : Activity(), SurfaceHolder.Callback, Choreographer.FrameCallback {

    private var handle: Long = 0
    private var displayScale: Float = 1f
    private var surfaceReady = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        System.loadLibrary("counter_example")

        displayScale = resources.displayMetrics.density

        val surfaceView = SurfaceView(this)
        surfaceView.holder.addCallback(this)
        setContentView(surfaceView)
    }

    // ── SurfaceHolder.Callback ──────────────────────────────────────────────

    override fun surfaceCreated(holder: SurfaceHolder) {
        // Surface is ready — create the iced rendering context
        val surface = holder.surface
        val frame = holder.surfaceFrame
        val savedState = getPreferences(MODE_PRIVATE).getString("counter_state", null)

        handle = NativeBridge.create(
            surface,
            frame.width(),
            frame.height(),
            displayScale,
            savedState ?: ""
        )

        if (handle != 0L) {
            surfaceReady = true
            Choreographer.getInstance().postFrameCallback(this)
        }
    }

    override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
        if (handle != 0L) {
            NativeBridge.resize(handle, width, height, displayScale)
        }
    }

    override fun surfaceDestroyed(holder: SurfaceHolder) {
        surfaceReady = false

        // Persist state before destroying
        if (handle != 0L) {
            val state = NativeBridge.getState(handle) as? String
            if (state != null) {
                getPreferences(MODE_PRIVATE).edit().putString("counter_state", state).apply()
            }
            NativeBridge.destroy(handle)
            handle = 0
        }
    }

    // ── Choreographer.FrameCallback ─────────────────────────────────────────

    override fun doFrame(frameTimeNanos: Long) {
        if (!surfaceReady || handle == 0L) return

        val needsRedraw = NativeBridge.enterFrame(handle)

        // Always schedule the next frame for simplicity.
        // For power efficiency, check needsRedraw and only schedule if > 0.
        Choreographer.getInstance().postFrameCallback(this)
    }

    // ── Touch events ────────────────────────────────────────────────────────

    override fun onTouchEvent(event: MotionEvent): Boolean {
        if (handle == 0L) return super.onTouchEvent(event)

        val pointerIndex = event.actionIndex
        val pointerId = event.getPointerId(pointerIndex)
        val action = when (event.actionMasked) {
            MotionEvent.ACTION_DOWN, MotionEvent.ACTION_POINTER_DOWN -> 0
            MotionEvent.ACTION_UP, MotionEvent.ACTION_POINTER_UP -> 1
            MotionEvent.ACTION_MOVE -> 2
            else -> 3 // ACTION_CANCEL and others
        }

        if (event.actionMasked == MotionEvent.ACTION_MOVE) {
            // ACTION_MOVE reports all pointers — forward each one
            for (i in 0 until event.pointerCount) {
                NativeBridge.touch(
                    handle, action, event.getPointerId(i),
                    event.getX(i), event.getY(i), displayScale
                )
            }
        } else {
            NativeBridge.touch(
                handle, action, pointerId,
                event.getX(pointerIndex), event.getY(pointerIndex), displayScale
            )
        }

        return true
    }
}

// ── JNI bridge ──────────────────────────────────────────────────────────────

object NativeBridge {
    external fun create(
        surface: Surface,
        width: Int,
        height: Int,
        scaleFactor: Float,
        savedState: String
    ): Long

    external fun destroy(handle: Long)
    external fun enterFrame(handle: Long): Int
    external fun resize(handle: Long, width: Int, height: Int, scaleFactor: Float)
    external fun touch(handle: Long, action: Int, fingerId: Int, x: Float, y: Float, displayScale: Float)
    external fun getState(handle: Long): Any?
}
