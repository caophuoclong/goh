//! Wheel input over a click-through popup. The adapter remains the selection owner.
use goh_anchor::Rect;

#[derive(Default)]
struct Wheel {
    target: Option<(String, Rect)>,
    remainder: f64,
    pending: i32,
}

impl Wheel {
    fn target(&mut self, id: &str, bounds: Rect) {
        if self.target.as_ref().is_none_or(|(old, _)| old != id) {
            self.remainder = 0.0;
            self.pending = 0;
        }
        self.target = Some((id.to_owned(), bounds));
    }

    fn hide(&mut self) {
        *self = Self::default();
    }

    /// Native positive vertical deltas mean up. Continuous devices report points; wheels report
    /// detents. Return whether this event belongs to the popup, even below the one-row threshold.
    fn event(&mut self, x: f64, y: f64, vertical: f64, continuous: bool) -> bool {
        let Some((_, b)) = &self.target else {
            return false;
        };
        if !x.is_finite()
            || !y.is_finite()
            || !vertical.is_finite()
            || x < b.x
            || x >= b.x + b.w
            || y < b.y
            || y >= b.y + b.h
        {
            self.remainder = 0.0;
            return false;
        }
        // Horizontal-only gestures continue to the terminal.
        if vertical == 0.0 {
            return false;
        }
        let rows = if continuous {
            vertical / f64::from(goh_popup::model::ROW_HEIGHT)
        } else {
            vertical
        };
        self.remainder -= rows.clamp(-100.0, 100.0);
        let delta = self.remainder.trunc() as i32;
        self.remainder -= f64::from(delta);
        self.pending = (self.pending + delta).clamp(-100, 100);
        true
    }

    fn take(&mut self) -> Option<(String, i32)> {
        if self.pending == 0 {
            return None;
        }
        Some((
            self.target.as_ref()?.0.clone(),
            std::mem::take(&mut self.pending),
        ))
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use super::Wheel;
    use eframe::egui;
    use goh_anchor::Rect;
    use std::ffi::c_void;
    use std::ptr::null_mut;
    use std::sync::Mutex;

    type Ref = *mut c_void;
    const SCROLL: u32 = 22;
    #[repr(C)]
    struct Point {
        x: f64,
        y: f64,
    }

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXIsProcessTrusted() -> bool;
        fn CGEventTapCreate(
            location: u32,
            place: u32,
            options: u32,
            mask: u64,
            callback: extern "C" fn(Ref, u32, Ref, Ref) -> Ref,
            info: Ref,
        ) -> Ref;
        fn CGEventTapEnable(tap: Ref, enabled: bool);
        fn CGEventGetLocation(event: Ref) -> Point;
        fn CGEventGetIntegerValueField(event: Ref, field: u32) -> i64;
        fn CGEventGetDoubleValueField(event: Ref, field: u32) -> f64;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFMachPortCreateRunLoopSource(allocator: Ref, port: Ref, order: isize) -> Ref;
        fn CFMachPortInvalidate(port: Ref);
        fn CFRunLoopGetMain() -> Ref;
        fn CFRunLoopAddSource(run_loop: Ref, source: Ref, mode: Ref);
        fn CFRunLoopRemoveSource(run_loop: Ref, source: Ref, mode: Ref);
        fn CFRelease(object: Ref);
        static kCFRunLoopCommonModes: Ref;
    }

    struct Callback {
        wheel: Mutex<Wheel>,
        ctx: egui::Context,
        tap: Ref,
    }

    extern "C" fn callback(_proxy: Ref, kind: u32, event: Ref, info: Ref) -> Ref {
        // The box outlives its run-loop source; Drop invalidates the tap before freeing the box.
        let state = unsafe { &*(info as *const Callback) };
        if kind == 0xffff_fffe || kind == 0xffff_ffff {
            unsafe { CGEventTapEnable(state.tap, true) };
            return event;
        }
        if kind != SCROLL || event.is_null() {
            return event;
        }
        // A callback must never wait for the renderer or panic through the C ABI.
        let Ok(mut wheel) = state.wheel.try_lock() else {
            return event;
        };
        let point = unsafe { CGEventGetLocation(event) };
        let continuous = unsafe { CGEventGetIntegerValueField(event, 88) != 0 };
        let delta = unsafe {
            if continuous {
                CGEventGetDoubleValueField(event, 96)
            } else {
                CGEventGetIntegerValueField(event, 11) as f64
            }
        };
        if wheel.event(point.x, point.y, delta, continuous) {
            state.ctx.request_repaint();
            null_mut() // Do not scroll the terminal underneath this popup as well.
        } else {
            event
        }
    }

    /// Main-thread run-loop source, limited to wheel events. No mouse clicks or keyboard events
    /// are intercepted. The window stays click-through and cannot take focus on a click.
    pub struct Monitor {
        source: Ref,
        state: Box<Callback>,
    }

    impl Monitor {
        pub fn new(ctx: egui::Context) -> Option<Self> {
            // Use existing Accessibility authorization only; never prompt for additional access.
            if !unsafe { AXIsProcessTrusted() } {
                return None;
            }
            let mut state = Box::new(Callback {
                wheel: Mutex::new(Wheel::default()),
                ctx,
                tap: null_mut(),
            });
            unsafe {
                state.tap = CGEventTapCreate(
                    1,
                    0,
                    0,
                    1u64 << SCROLL,
                    callback,
                    (&mut *state as *mut Callback).cast(),
                );
                if state.tap.is_null() {
                    return None;
                }
                let source = CFMachPortCreateRunLoopSource(null_mut(), state.tap, 0);
                if source.is_null() {
                    CFMachPortInvalidate(state.tap);
                    CFRelease(state.tap);
                    return None;
                }
                CFRunLoopAddSource(CFRunLoopGetMain(), source, kCFRunLoopCommonModes);
                CGEventTapEnable(state.tap, true);
                Some(Self { source, state })
            }
        }

        pub fn target(&self, id: &str, bounds: Rect) {
            if let Ok(mut wheel) = self.state.wheel.lock() {
                wheel.target(id, bounds);
            }
        }
        pub fn hide(&self) {
            if let Ok(mut wheel) = self.state.wheel.lock() {
                wheel.hide();
            }
        }
        pub fn take(&self) -> Option<(String, i32)> {
            self.state.wheel.lock().ok()?.take()
        }
    }

    #[cfg(test)]
    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn CGEventCreateScrollWheelEvent(source: Ref, units: u32, wheels: u32, wheel1: i32, ...) -> Ref;
        fn CGEventSetLocation(event: Ref, point: Point);
    }

    #[test]
    fn native_callback_reads_wheel_fields_and_passes_other_events_through() {
        // Create a local event without posting it or installing a tap. This needs no permissions
        // and checks the real macOS field/coordinate ABI without interacting with the desktop.
        let mut state = Box::new(Callback {
            wheel: Mutex::new(Wheel::default()),
            ctx: egui::Context::default(),
            tap: null_mut(),
        });
        state.wheel.lock().unwrap().target(
            "native-menu",
            Rect {
                x: 10.0,
                y: 20.0,
                w: 200.0,
                h: 100.0,
            },
        );
        let info = (&mut *state as *mut Callback).cast();
        unsafe {
            let event = CGEventCreateScrollWheelEvent(null_mut(), 1, 1, -3i32);
            assert!(!event.is_null());
            CGEventSetLocation(event, Point { x: 20.0, y: 30.0 });
            assert_eq!(
                callback(null_mut(), 10, event, info),
                event,
                "keyboard events must pass unchanged"
            );
            assert!(callback(null_mut(), SCROLL, event, info).is_null());
            assert_eq!(
                state.wheel.lock().unwrap().take(),
                Some(("native-menu".into(), 3))
            );
            CGEventSetLocation(event, Point { x: 500.0, y: 30.0 });
            assert_eq!(
                callback(null_mut(), SCROLL, event, info),
                event,
                "outside scrolling belongs to the terminal"
            );
            assert!(state.wheel.lock().unwrap().take().is_none());
            CFRelease(event);
        }
    }

    impl Drop for Monitor {
        fn drop(&mut self) {
            unsafe {
                CFMachPortInvalidate(self.state.tap);
                CFRunLoopRemoveSource(CFRunLoopGetMain(), self.source, kCFRunLoopCommonModes);
                CFRelease(self.source);
                CFRelease(self.state.tap);
            }
        }
    }
}

#[cfg(target_os = "macos")]
pub use mac::Monitor;

#[cfg(test)]
mod tests {
    use super::*;
    fn wheel() -> Wheel {
        let mut w = Wheel::default();
        w.target(
            "menu-1",
            Rect {
                x: 10.0,
                y: 20.0,
                w: 200.0,
                h: 100.0,
            },
        );
        w
    }
    #[test]
    fn only_wheel_inside_visible_popup_is_consumed() {
        let mut w = wheel();
        assert!(!w.event(9.0, 30.0, -1.0, false));
        assert!(!w.event(210.0, 30.0, -1.0, false));
        assert!(!w.event(20.0, 120.0, -1.0, false));
        assert!(!w.event(20.0, 30.0, f64::NAN, false));
        assert!(!w.event(20.0, 30.0, 0.0, false));
        assert!(w.take().is_none());
        assert!(w.event(20.0, 30.0, -3.0, false));
        assert_eq!(w.take(), Some(("menu-1".into(), 3)));
        w.hide();
        assert!(!w.event(20.0, 30.0, -1.0, false));
    }
    #[test]
    fn trackpad_accumulates_points_and_keeps_remainder_on_selection_updates() {
        let mut w = wheel();
        assert!(w.event(20.0, 30.0, -10.0, true));
        assert!(w.take().is_none());
        let bounds = w.target.as_ref().unwrap().1;
        w.target("menu-1", bounds);
        w.event(20.0, 30.0, -20.0, true);
        assert_eq!(w.take(), Some(("menu-1".into(), 1)));
        w.event(20.0, 30.0, 30.0, true);
        assert_eq!(w.take(), Some(("menu-1".into(), -1)));
    }
    #[test]
    fn new_query_discards_old_pending_scroll_and_large_deltas_are_bounded() {
        let mut w = wheel();
        w.event(20.0, 30.0, -4.0, false);
        let bounds = w.target.as_ref().unwrap().1;
        w.target("menu-2", bounds);
        assert!(w.take().is_none());
        w.event(20.0, 30.0, -1e20, true);
        w.event(20.0, 30.0, -1e20, false);
        assert_eq!(w.take(), Some(("menu-2".into(), 100)));
    }
}
