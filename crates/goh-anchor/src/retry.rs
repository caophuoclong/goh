//! Bounded retry for calls that fail transiently right after process start.

use std::time::Duration;

/// Call `f` up to `attempts` times, waiting `delay` between failures; returns the first success or
/// the last error. Never sleeps after a success or after the final attempt.
pub fn retry<T, E>(attempts: usize, delay: Duration, mut f: impl FnMut() -> Result<T, E>) -> Result<T, E> {
    let mut result = f();
    for _ in 1..attempts.max(1) {
        if result.is_ok() {
            break;
        }
        std::thread::sleep(delay);
        result = f();
    }
    result
}

/// Re-read until two consecutive reads agree, so a value sampled while something is redrawing (a
/// terminal cursor that is briefly one row off) is not trusted. Up to `max_extra` further reads,
/// `delay` apart. Returns the last value and whether it settled.
pub fn settle_from<T: PartialEq + Copy>(first: T, max_extra: usize, delay: Duration, mut read: impl FnMut() -> Option<T>) -> (T, bool) {
    let mut prev = first;
    for _ in 0..max_extra {
        std::thread::sleep(delay);
        match read() {
            Some(cur) if cur == prev => return (prev, true),
            Some(cur) => prev = cur,
            None => return (prev, false),
        }
    }
    (prev, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn succeeds_on_a_later_attempt() {
        let mut calls = 0;
        let r = retry(5, Duration::ZERO, || {
            calls += 1;
            if calls < 3 { Err(calls) } else { Ok("ok") }
        });
        assert_eq!((r, calls), (Ok("ok"), 3));
    }

    #[test]
    fn gives_up_with_the_last_error_after_the_limit() {
        let mut calls = 0;
        let r: Result<(), i32> = retry(4, Duration::ZERO, || {
            calls += 1;
            Err(calls)
        });
        assert_eq!((r, calls), (Err(4), 4));
    }

    #[test]
    fn a_first_success_does_not_retry_or_sleep() {
        let start = std::time::Instant::now();
        let mut calls = 0;
        let r: Result<i32, ()> = retry(5, Duration::from_secs(10), || {
            calls += 1;
            Ok(7)
        });
        assert_eq!((r, calls), (Ok(7), 1));
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn zero_attempts_still_calls_once() {
        let mut calls = 0;
        let _ = retry::<(), ()>(0, Duration::ZERO, || {
            calls += 1;
            Err(())
        });
        assert_eq!(calls, 1);
    }

    #[test]
    fn settle_ignores_a_transient_value() {
        // The cursor is briefly one row lower (as during a redraw), then returns.
        let mut reads = vec![Some(10), Some(10)].into_iter();
        let (v, settled) = settle_from(29, 4, Duration::ZERO, || reads.next().flatten());
        // first=29 (transient) differs from the next read (10), which then repeats: settles on 10.
        assert_eq!((v, settled), (10, true));
    }

    #[test]
    fn settle_reports_a_value_that_never_stops_moving() {
        let mut n = 0;
        let (v, settled) = settle_from(0, 3, Duration::ZERO, || {
            n += 1;
            Some(n)
        });
        assert_eq!((v, settled), (3, false));
    }

    #[test]
    fn settle_returns_at_once_when_stable_and_survives_a_failed_read() {
        let mut calls = 0;
        assert_eq!(settle_from(7, 4, Duration::ZERO, || { calls += 1; Some(7) }), (7, true));
        assert_eq!(calls, 1);
        assert_eq!(settle_from(7, 4, Duration::ZERO, || None::<i32>), (7, false));
    }
}
