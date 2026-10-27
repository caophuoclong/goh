//! While a menu is open the popup re-checks the terminal periodically, so it never outlives the
//! situation it was shown for: if the terminal is minimised or hidden, another app takes focus, or the
//! caret can no longer be read, the overlay goes away; if the window moves, it follows; and if the
//! terminal comes back while the menu is still open, it returns.

use crate::placement::Placement;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickAction {
    Nothing,
    Hide,
    Show,
    Move,
}

/// A position that moved less than this (in points) is the same position.
const MOVED: f64 = 0.5;

/// `now` is where the overlay should be right now, or `None` when it cannot be shown at all.
pub fn plan_tick(visible: bool, at: Option<Placement>, now: Option<Placement>) -> TickAction {
    match (visible, now) {
        (true, None) => TickAction::Hide,
        (false, None) => TickAction::Nothing,
        (false, Some(_)) => TickAction::Show,
        (true, Some(n)) => match at {
            Some(a) if (a.x - n.x).abs() <= MOVED && (a.y - n.y).abs() <= MOVED && a.above == n.above => TickAction::Nothing,
            _ => TickAction::Move,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> Placement {
        Placement { x, y, above: false }
    }

    #[test]
    fn hides_when_the_terminal_stops_being_usable() {
        assert_eq!(plan_tick(true, Some(p(1.0, 2.0)), None), TickAction::Hide);
    }

    #[test]
    fn stays_hidden_while_it_cannot_be_shown() {
        assert_eq!(plan_tick(false, None, None), TickAction::Nothing);
    }

    #[test]
    fn comes_back_when_the_terminal_does() {
        assert_eq!(plan_tick(false, None, Some(p(1.0, 2.0))), TickAction::Show);
    }

    #[test]
    fn follows_a_moved_window_but_ignores_jitter() {
        let at = Some(p(100.0, 200.0));
        assert_eq!(plan_tick(true, at, Some(p(100.0, 200.0))), TickAction::Nothing);
        assert_eq!(plan_tick(true, at, Some(p(100.3, 199.8))), TickAction::Nothing);
        assert_eq!(plan_tick(true, at, Some(p(540.0, 200.0))), TickAction::Move);
        assert_eq!(plan_tick(true, at, Some(p(100.0, 219.0))), TickAction::Move);
        assert_eq!(plan_tick(true, at, Some(Placement { above: true, ..p(100.0, 200.0) })), TickAction::Move);
    }
}
