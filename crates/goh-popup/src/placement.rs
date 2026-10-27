//! Where the popup window goes relative to the caret, on the display that holds the caret.
//! Global screen points, top-left origin (the space Accessibility rectangles and winit use).

use goh_anchor::{Caret, Rect};

/// Distance between the caret line and the popup (design token space-3).
pub const GAP: f64 = 12.0;
/// Keep the popup this far from the display edge.
pub const MARGIN: f64 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub x: f64,
    pub y: f64,
    /// The popup is above the caret line because there was not room below.
    pub above: bool,
}

/// Horizontal room the popup can use on `display`: it may shift left, so the whole display width
/// minus margins counts, not just the space right of the caret.
pub fn available_width(display: Rect) -> f64 {
    (display.w - 2.0 * MARGIN).max(0.0)
}

pub fn place(caret: Caret, size: (f64, f64), display: Rect) -> Placement {
    let (w, h) = size;
    let below = caret.y + caret.height + GAP;
    let above_y = caret.y - GAP - h;
    let fits_below = below + h <= display.bottom() - MARGIN;
    let fits_above = above_y >= display.y + MARGIN;
    // Below by default; above only when that is the side with room.
    let (mut y, above) = if fits_below || !fits_above { (below, false) } else { (above_y, true) };
    // A popup taller than either side is pinned inside the display rather than pushed off it.
    y = y.min(display.bottom() - MARGIN - h).max(display.y + MARGIN);
    let x = caret.x.min(display.right() - MARGIN - w).max(display.x + MARGIN);
    Placement { x, y, above }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect { x: 0.0, y: 0.0, w: 1512.0, h: 982.0 };
    const SIZE: (f64, f64) = (400.0, 250.0);

    fn caret(x: f64, y: f64) -> Caret {
        Caret { x, y, height: 19.0 }
    }

    #[test]
    fn normally_below_the_caret_line() {
        let p = place(caret(100.0, 200.0), SIZE, SCREEN);
        assert_eq!(p, Placement { x: 100.0, y: 200.0 + 19.0 + GAP, above: false });
    }

    #[test]
    fn flips_above_near_the_bottom_edge() {
        // Last terminal row: nothing fits below.
        let p = place(caret(100.0, 923.0), SIZE, SCREEN);
        assert!(p.above);
        assert_eq!(p.y, 923.0 - GAP - SIZE.1);
        assert!(p.y + SIZE.1 <= 923.0, "does not cover the caret line");
    }

    #[test]
    fn clamps_to_the_right_edge_and_never_goes_left_of_the_display() {
        assert_eq!(place(caret(1400.0, 100.0), SIZE, SCREEN).x, 1512.0 - MARGIN - 400.0);
        assert_eq!(place(caret(-50.0, 100.0), SIZE, SCREEN).x, MARGIN);
    }

    #[test]
    fn uses_the_display_that_holds_the_caret() {
        // Second display to the right with a different origin and size (as in the S-04 runs).
        let second = Rect { x: 1512.0, y: 0.0, w: 1600.0, h: 1000.0 };
        let p = place(caret(3000.0, 300.0), SIZE, second);
        assert_eq!(p.x, 3112.0 - MARGIN - 400.0);
        assert!(p.x >= second.x && p.x + SIZE.0 <= second.right());
        assert_eq!(available_width(second), 1600.0 - 16.0);
    }

    #[test]
    fn popup_taller_than_the_display_is_pinned_inside_it() {
        let tiny = Rect { x: 0.0, y: 0.0, w: 800.0, h: 200.0 };
        let p = place(caret(10.0, 100.0), (400.0, 300.0), tiny);
        assert!(p.y >= tiny.y, "{p:?}");
    }

    #[test]
    fn never_leaves_the_display_when_it_fits() {
        for cy in [0.0, 100.0, 500.0, 800.0, 960.0] {
            for cx in [0.0, 700.0, 1500.0] {
                let p = place(caret(cx, cy), SIZE, SCREEN);
                assert!(p.x >= SCREEN.x && p.x + SIZE.0 <= SCREEN.right(), "{cx},{cy}: {p:?}");
                assert!(p.y >= SCREEN.y && p.y + SIZE.1 <= SCREEN.bottom(), "{cx},{cy}: {p:?}");
            }
        }
    }
}
