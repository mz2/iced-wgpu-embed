//! Touch event types and translation to iced events.
//!
//! Provides [`TouchPhase`] and [`TouchEvent`] as `#[repr(C)]` types suitable
//! for C FFI, plus [`translate_touch_events`] to convert a batch of touch
//! events into iced [`Event`](iced_core::Event) values.
//!
//! Coordinates are expected in **logical points** (not physical pixels).
//! On platforms like Android where touch events arrive in physical pixels,
//! the caller must divide by the display scale before passing them here.

use iced_core::event::Event;
use iced_core::touch;
use iced_core::Point;

/// Touch event phase, matching UITouch.Phase (iOS) and MotionEvent actions (Android).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchPhase {
    /// Finger touched the surface (ACTION_DOWN / UITouch.Phase.began).
    Started = 0,
    /// Finger moved on the surface (ACTION_MOVE / UITouch.Phase.moved).
    Moved = 1,
    /// Finger lifted from the surface (ACTION_UP / UITouch.Phase.ended).
    Ended = 2,
    /// Touch was cancelled by the system (ACTION_CANCEL / UITouch.Phase.cancelled).
    Cancelled = 3,
}

/// A single touch event in logical coordinates.
///
/// This struct is `#[repr(C)]` so it can be passed directly from C/Swift FFI
/// code as a contiguous array.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TouchEvent {
    /// The phase of this touch event.
    pub phase: TouchPhase,
    /// Unique identifier for the finger (stable across a press-move-lift sequence).
    pub finger_id: u64,
    /// X position in logical points.
    pub x: f32,
    /// Y position in logical points.
    pub y: f32,
}

/// Convert a slice of [`TouchEvent`]s to a `Vec` of iced [`Event`]s.
///
/// Each touch event is translated to the corresponding
/// [`iced_core::touch::Event`] variant. Coordinates are passed through
/// as-is (they must already be in logical points).
pub fn translate_touch_events(events: &[TouchEvent]) -> Vec<Event> {
    events.iter().filter_map(translate_one).collect()
}

fn translate_one(e: &TouchEvent) -> Option<Event> {
    let id = touch::Finger(e.finger_id);
    let position = Point::new(e.x, e.y);

    let touch_event = match e.phase {
        TouchPhase::Started => touch::Event::FingerPressed { id, position },
        TouchPhase::Moved => touch::Event::FingerMoved { id, position },
        TouchPhase::Ended => touch::Event::FingerLifted { id, position },
        TouchPhase::Cancelled => touch::Event::FingerLost { id, position },
    };

    Some(Event::Touch(touch_event))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate_finger_pressed() {
        let events = [TouchEvent {
            phase: TouchPhase::Started,
            finger_id: 42,
            x: 100.0,
            y: 200.0,
        }];
        let result = translate_touch_events(&events);
        assert_eq!(result.len(), 1);
        match &result[0] {
            Event::Touch(touch::Event::FingerPressed { id, position }) => {
                assert_eq!(id.0, 42);
                assert_eq!(position.x, 100.0);
                assert_eq!(position.y, 200.0);
            }
            other => panic!("Expected FingerPressed, got {other:?}"),
        }
    }

    #[test]
    fn translate_finger_moved() {
        let events = [TouchEvent {
            phase: TouchPhase::Moved,
            finger_id: 7,
            x: 150.0,
            y: 250.0,
        }];
        let result = translate_touch_events(&events);
        assert_eq!(result.len(), 1);
        match &result[0] {
            Event::Touch(touch::Event::FingerMoved { id, position }) => {
                assert_eq!(id.0, 7);
                assert_eq!(position.x, 150.0);
                assert_eq!(position.y, 250.0);
            }
            other => panic!("Expected FingerMoved, got {other:?}"),
        }
    }

    #[test]
    fn translate_finger_lifted() {
        let events = [TouchEvent {
            phase: TouchPhase::Ended,
            finger_id: 99,
            x: 50.0,
            y: 75.0,
        }];
        let result = translate_touch_events(&events);
        assert_eq!(result.len(), 1);
        match &result[0] {
            Event::Touch(touch::Event::FingerLifted { id, position }) => {
                assert_eq!(id.0, 99);
                assert_eq!(position.x, 50.0);
                assert_eq!(position.y, 75.0);
            }
            other => panic!("Expected FingerLifted, got {other:?}"),
        }
    }

    #[test]
    fn translate_finger_lost() {
        let events = [TouchEvent {
            phase: TouchPhase::Cancelled,
            finger_id: 1,
            x: 0.0,
            y: 0.0,
        }];
        let result = translate_touch_events(&events);
        assert_eq!(result.len(), 1);
        match &result[0] {
            Event::Touch(touch::Event::FingerLost { id, .. }) => {
                assert_eq!(id.0, 1);
            }
            other => panic!("Expected FingerLost, got {other:?}"),
        }
    }

    #[test]
    fn translate_multiple_events() {
        let events = [
            TouchEvent { phase: TouchPhase::Started, finger_id: 1, x: 10.0, y: 20.0 },
            TouchEvent { phase: TouchPhase::Started, finger_id: 2, x: 30.0, y: 40.0 },
            TouchEvent { phase: TouchPhase::Moved, finger_id: 1, x: 15.0, y: 25.0 },
        ];
        let result = translate_touch_events(&events);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn translate_empty_events() {
        let result = translate_touch_events(&[]);
        assert!(result.is_empty());
    }
}
