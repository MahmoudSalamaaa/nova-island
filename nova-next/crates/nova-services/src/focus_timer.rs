//! Pure Focus Timer state machine. UI/bridge integration is intentionally separate.
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Session { Focus, Break }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status { Idle, Running, Paused, Completed }

#[derive(Debug)]
pub struct FocusTimer {
    session: Session,
    status: Status,
    focus: Duration,
    rest: Duration,
    remaining: Duration,
    deadline: Option<Instant>,
}

impl FocusTimer {
    pub fn new(focus_minutes: u32, break_minutes: u32) -> Result<Self, &'static str> {
        if !(5..=120).contains(&focus_minutes) || !(1..=30).contains(&break_minutes) {
            return Err("duration out of bounds");
        }
        let focus = Duration::from_secs(u64::from(focus_minutes) * 60);
        let rest = Duration::from_secs(u64::from(break_minutes) * 60);
        Ok(Self { session: Session::Focus, status: Status::Idle, focus, rest, remaining: focus, deadline: None })
    }

    pub fn session(&self) -> Session { self.session }
    pub fn status(&self) -> Status { self.status }

    pub fn remaining_at(&self, now: Instant) -> Duration {
        match self.deadline {
            Some(deadline) if self.status == Status::Running => deadline.saturating_duration_since(now),
            _ => self.remaining,
        }
    }

    pub fn start(&mut self, now: Instant) {
        if matches!(self.status, Status::Idle | Status::Paused) {
            self.deadline = Some(now + self.remaining);
            self.status = Status::Running;
        }
    }

    pub fn pause(&mut self, now: Instant) {
        if self.status == Status::Running {
            self.remaining = self.remaining_at(now);
            self.deadline = None;
            self.status = if self.remaining.is_zero() { Status::Completed } else { Status::Paused };
        }
    }

    /// Returns true only once when a running session completes.
    pub fn tick(&mut self, now: Instant) -> bool {
        if self.status == Status::Running && self.remaining_at(now).is_zero() {
            self.remaining = Duration::ZERO;
            self.deadline = None;
            self.status = Status::Completed;
            return true;
        }
        false
    }

    pub fn reset(&mut self) {
        self.remaining = self.duration_for(self.session);
        self.deadline = None;
        self.status = Status::Idle;
    }

    pub fn switch_to(&mut self, session: Session) {
        self.session = session;
        self.reset();
    }

    fn duration_for(&self, session: Session) -> Duration {
        match session { Session::Focus => self.focus, Session::Break => self.rest }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_pause_resume_complete_once() {
        let t = Instant::now();
        let mut timer = FocusTimer::new(25, 5).unwrap();
        timer.start(t);
        assert_eq!(timer.remaining_at(t + Duration::from_secs(30)), Duration::from_secs(1470));
        timer.pause(t + Duration::from_secs(30));
        assert_eq!(timer.status(), Status::Paused);
        timer.start(t + Duration::from_secs(100));
        assert!(!timer.tick(t + Duration::from_secs(1569)));
        assert!(timer.tick(t + Duration::from_secs(1570)));
        assert!(!timer.tick(t + Duration::from_secs(1571)));
        assert_eq!(timer.status(), Status::Completed);
    }

    #[test]
    fn reset_and_switch() {
        let mut timer = FocusTimer::new(25, 5).unwrap();
        timer.switch_to(Session::Break);
        assert_eq!(timer.remaining_at(Instant::now()), Duration::from_secs(300));
        timer.reset();
        assert_eq!(timer.status(), Status::Idle);
    }

    #[test]
    fn invalid_durations() {
        assert!(FocusTimer::new(4, 5).is_err());
        assert!(FocusTimer::new(25, 31).is_err());
        assert!(FocusTimer::new(120, 1).is_ok());
    }
}
