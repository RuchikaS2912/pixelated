//! Pure animation math — fully unit-testable, no GUI types.
//!
//! A `WalkPlan` describes one full walk across a screen band:
//! start position -> end position, which sprite frame at which time,
//! and when the overlay should dismiss itself.

use crate::character::Direction;

pub const MIN_DURATION_SECS: f64 = 6.0;
pub const MAX_DURATION_SECS: f64 = 24.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WalkPlan {
    /// Screen-space x where the character center starts (off-screen).
    pub start_x: f64,
    /// Screen-space x where the character center ends (off-screen).
    pub end_x: f64,
    /// Vertical center line for the character (screen coords).
    pub y: f64,
    pub duration: f64,
    /// Effective walking speed px/s (after duration fitting).
    pub speed: f64,
    pub frame_rate: f64,
    pub frame_count: usize,
    pub direction: Direction,
}

impl WalkPlan {
    /// Build a plan crossing `screen_width` within `[MIN, MAX]` seconds,
    /// preferring `desired_duration` / `desired_speed` when compatible.
    ///
    /// `margin` is how far off-screen the character center starts/ends
    /// (the caller passes the overlay window width, so the character is
    /// fully hidden on both sides without walking more than necessary).
    pub fn new(
        screen_width: f64,
        band_y: f64,
        margin: f64,
        direction: Direction,
        frame_rate: f64,
        frame_count: usize,
        desired_speed: f64,
        desired_duration: Option<f64>,
    ) -> WalkPlan {
        let margin = margin.max(200.0);
        let (start_x, end_x) = match direction {
            Direction::Right => (-margin, screen_width + margin),
            Direction::Left => (screen_width + margin, -margin),
        };
        let distance = (end_x - start_x).abs();

        let mut duration = desired_duration.unwrap_or(distance / desired_speed.max(1.0));
        if desired_duration.is_none() {
            duration = duration.clamp(MIN_DURATION_SECS, MAX_DURATION_SECS);
        }
        duration = duration.clamp(1.0, 60.0);
        let speed = distance / duration;

        WalkPlan {
            start_x,
            end_x,
            y: band_y,
            duration,
            speed,
            frame_rate: frame_rate.max(1.0),
            frame_count: frame_count.max(1),
            direction,
        }
    }

    /// Character center x at time `t`.
    pub fn x_at(&self, t: f64) -> f64 {
        let frac = (t / self.duration).clamp(0.0, 1.0);
        self.start_x + (self.end_x - self.start_x) * frac
    }

    /// Sprite frame index at time `t` (walking = animating).
    pub fn frame_at(&self, t: f64) -> usize {
        let idx = (t * self.frame_rate).floor() as usize;
        idx % self.frame_count
    }

    /// True once the character has fully exited the far edge.
    pub fn done(&self, t: f64) -> bool {
        t >= self.duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crosses_screen_left_to_right() {
        let p = WalkPlan::new(1440.0, 900.0, 300.0, Direction::Right, 10.0, 6, 180.0, None);
        assert!(p.start_x < 0.0);
        assert!(p.end_x > 1440.0);
        let mid = p.x_at(p.duration / 2.0);
        assert!((mid - 720.0).abs() < 1.0, "mid should be ~center, got {mid}");
        assert!(!p.done(p.duration - 0.01));
        assert!(p.done(p.duration));
    }

    #[test]
    fn duration_clamped_for_tiny_screens() {
        // Very fast walk on small screen still lasts >= 6s by default.
        let p = WalkPlan::new(500.0, 400.0, 300.0, Direction::Left, 10.0, 6, 10000.0, None);
        assert!(p.duration >= MIN_DURATION_SECS - 1e-9);
    }

    #[test]
    fn explicit_duration_respected() {
        let p = WalkPlan::new(1440.0, 900.0, 300.0, Direction::Right, 10.0, 6, 180.0, Some(3.0));
        assert_eq!(p.duration, 3.0);
        // Speed auto-adjusted so the character still exits the screen.
        assert!(p.speed > 180.0);
    }

    #[test]
    fn frames_cycle() {
        let p = WalkPlan::new(1000.0, 500.0, 300.0, Direction::Right, 10.0, 6, 180.0, None);
        assert_eq!(p.frame_at(0.0), 0);
        assert_eq!(p.frame_at(0.1), 1);
        assert_eq!(p.frame_at(0.6), 6 % 6);
        assert_eq!(p.frame_at(0.7), 7 % 6);
    }

    #[test]
    fn default_speed_gives_leisurely_walk() {
        // 1440px screen + 2x300 margin = 2040px at the default 110 px/s
        // -> ~18.5s stroll, within the 6..24s clamp (no re-speeding).
        let p = WalkPlan::new(1440.0, 900.0, 300.0, Direction::Right, 10.0, 6, 110.0, None);
        assert!((p.duration - 2040.0 / 110.0).abs() < 0.01, "duration {}", p.duration);
        assert!((p.speed - 110.0).abs() < 0.01);
    }

    #[test]
    fn margin_floored() {
        // Tiny window still hides the character fully off-screen.
        let p = WalkPlan::new(1000.0, 500.0, 50.0, Direction::Right, 10.0, 6, 10000.0, None);
        assert!(p.start_x <= -200.0);
        assert!(p.end_x >= 1200.0);
    }

    #[test]
    fn left_direction_reversed() {
        let p = WalkPlan::new(1000.0, 500.0, 300.0, Direction::Left, 10.0, 6, 180.0, None);
        assert!(p.start_x > 1000.0);
        assert!(p.end_x < 0.0);
        assert!(p.x_at(p.duration / 2.0) < p.start_x);
    }
}
