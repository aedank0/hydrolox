use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct Timer {
    start: Instant,
    duration: Duration,
}
impl Timer {
    pub fn from_now(duration: Duration) -> Self {
        Self {
            start: Instant::now(),
            duration,
        }
    }
    pub fn end(&self) -> Instant {
        self.start + self.duration
    }
    /// Returns true if the timer's duration has passed, and returns false otherwise.
    pub fn check(&self) -> bool {
        let now = Instant::now();
        if now >= self.end() {
            true
        } else {
            false
        }
    }
    /// Returns true and resets the timer if its duration has passed, and returns false otherwise.
    pub fn check_reset(&mut self) -> bool {
        let now = Instant::now();
        if now >= self.end() {
            self.start = now;
            true
        } else {
            false
        }
    }
}

#[derive(Debug)]
pub struct Stopwatch {
    total: Duration,
    last_start: Option<Instant>,
}
impl Stopwatch {
    pub fn since_last_start(&self) -> Option<Duration> {
        Some(Instant::now().duration_since(self.last_start?))
    }
    pub fn total(&mut self) -> Duration {
        let mut sum = self.total;
        if let Some(dur) = self.since_last_start() {
            sum += dur;
        }
        sum
    }
    pub fn pause(&mut self) {
        if let Some(since) = self.since_last_start() {
            self.total += since;
        }
        self.last_start = None;
    }
    pub fn reset(&mut self) {
        self.total = Duration::new(0, 0);
        if self.last_start.is_some() {
            self.last_start = Some(Instant::now())
        }
    }
}
