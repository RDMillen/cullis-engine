//! The fixed simulation clock: the game advances in ticks of [`TICK`] seconds
//! whatever the frame rate, so its logic is deterministic and frame-rate
//! independent; rendering interpolates between the last two ticks by
//! [`Clock::alpha`].

/// Simulation ticks per second.
pub const RATE: u32 = 30;
/// Seconds per tick.
pub const TICK: f32 = 1.0 / RATE as f32;
/// Most ticks run for one frame: after a long stall the game slows down rather
/// than running many ticks to catch up.
const MAX_TICKS: u32 = 5;

/// Turns frame times into simulation ticks.
#[derive(Debug, Clone, Default)]
pub struct Clock {
    /// Frame time not yet spent on ticks, in seconds (less than a tick after
    /// [`Clock::advance`]).
    owed: f32,
    /// Ticks run since the clock started.
    ticks: u64,
}

impl Clock {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a frame of `dt` seconds and returns how many ticks to run now.
    pub fn advance(&mut self, dt: f32) -> u32 {
        self.owed += dt.max(0.0);
        let mut due = 0;
        while self.owed >= TICK && due < MAX_TICKS {
            self.owed -= TICK;
            due += 1;
        }
        if due == MAX_TICKS {
            self.owed = self.owed.min(TICK);
        }
        self.ticks += u64::from(due);
        due
    }

    /// How far the frame is between the last tick and the next (0..1), for
    /// interpolating what is drawn.
    pub fn alpha(&self) -> f32 {
        (self.owed / TICK).clamp(0.0, 1.0)
    }

    /// Ticks run since the clock started.
    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    /// Simulated seconds since the clock started.
    pub fn seconds(&self) -> f64 {
        self.ticks as f64 / f64::from(RATE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_become_ticks() {
        let mut c = Clock::new();
        // 60 frames a second: a tick every other frame.
        let ticks: u32 = (0..60).map(|_| c.advance(1.0 / 60.0)).sum();
        assert!((29..=31).contains(&ticks), "{ticks}");
        // 20 frames a second: one or two ticks a frame.
        let mut c = Clock::new();
        let per_frame: Vec<u32> = (0..4).map(|_| c.advance(1.0 / 20.0)).collect();
        // 0.2 s is six ticks, give or take rounding.
        assert!(
            (5..=6).contains(&per_frame.iter().sum::<u32>()),
            "{per_frame:?}"
        );
        assert!(per_frame.iter().all(|&t| t <= 2));
        assert!(c.alpha() < 1.0);
    }

    #[test]
    fn a_stall_does_not_run_away() {
        let mut c = Clock::new();
        assert_eq!(c.advance(10.0), MAX_TICKS);
        let after = c.advance(0.0) + c.advance(0.0);
        assert!(after <= 1, "the backlog is dropped: {after}");
        assert_eq!(c.ticks(), u64::from(MAX_TICKS + after));
    }
}
