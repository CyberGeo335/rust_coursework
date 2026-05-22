use std::fmt;
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmployeeId(String);

impl EmployeeId {
    pub fn parse(value: impl Into<String>) -> Result<Self, &'static str> {
        let value = value.into();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err("employee id is empty");
        }
        if trimmed.len() > 64 {
            return Err("employee id is too long");
        }
        if !trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        {
            return Err("employee id contains unsupported characters");
        }
        Ok(Self(trimmed.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityKind {
    Keyboard,
    Mouse,
    WindowFocus,
    Heartbeat,
}

impl fmt::Display for ActivityKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            ActivityKind::Keyboard => "keyboard",
            ActivityKind::Mouse => "mouse",
            ActivityKind::WindowFocus => "window_focus",
            ActivityKind::Heartbeat => "heartbeat",
        };
        f.write_str(value)
    }
}

#[derive(Debug, Clone)]
pub struct ActivityEvent {
    pub employee_id: EmployeeId,
    pub kind: ActivityKind,
    pub occurred_at: SystemTime,
}

impl ActivityEvent {
    pub fn new(employee_id: EmployeeId, kind: ActivityKind) -> Self {
        Self {
            employee_id,
            kind,
            occurred_at: SystemTime::now(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ActivitySnapshot {
    pub employee_id: EmployeeId,
    pub active_seconds: u64,
    pub idle_seconds: u64,
    pub total_events: u64,
    pub generated_at: SystemTime,
}

impl ActivitySnapshot {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"employee_id\":\"{}\",\"active_seconds\":{},\"idle_seconds\":{},\"total_events\":{},\"generated_at_unix\":{}}}",
            escape_json(self.employee_id.as_str()),
            self.active_seconds,
            self.idle_seconds,
            self.total_events,
            unix_seconds(self.generated_at)
        )
    }
}

#[derive(Debug)]
pub struct ActivityAccumulator {
    employee_id: EmployeeId,
    idle_after: Duration,
    last_activity_at: Option<SystemTime>,
    last_tick_at: SystemTime,
    active: Duration,
    idle: Duration,
    total_events: u64,
}

impl ActivityAccumulator {
    pub fn new(employee_id: EmployeeId, idle_after: Duration, now: SystemTime) -> Self {
        Self {
            employee_id,
            idle_after,
            last_activity_at: None,
            last_tick_at: now,
            active: Duration::ZERO,
            idle: Duration::ZERO,
            total_events: 0,
        }
    }

    pub fn record(&mut self, event: ActivityEvent) {
        if event.employee_id == self.employee_id {
            self.roll_forward(event.occurred_at);
            self.last_activity_at = Some(event.occurred_at);
            self.total_events = self.total_events.saturating_add(1);
        }
    }

    pub fn snapshot(&mut self, now: SystemTime) -> ActivitySnapshot {
        self.roll_forward(now);
        ActivitySnapshot {
            employee_id: self.employee_id.clone(),
            active_seconds: self.active.as_secs(),
            idle_seconds: self.idle.as_secs(),
            total_events: self.total_events,
            generated_at: now,
        }
    }

    fn roll_forward(&mut self, now: SystemTime) {
        if now <= self.last_tick_at {
            return;
        }
        let delta = now
            .duration_since(self.last_tick_at)
            .unwrap_or(Duration::ZERO);
        match self.last_activity_at {
            None => self.idle = self.idle.saturating_add(delta),
            Some(last_activity) => {
                let idle_started_at = last_activity + self.idle_after;
                if self.last_tick_at >= idle_started_at {
                    self.idle = self.idle.saturating_add(delta);
                } else if now <= idle_started_at {
                    self.active = self.active.saturating_add(delta);
                } else {
                    let active_delta = idle_started_at
                        .duration_since(self.last_tick_at)
                        .unwrap_or(Duration::ZERO);
                    let idle_delta = now
                        .duration_since(idle_started_at)
                        .unwrap_or(Duration::ZERO);
                    self.active = self.active.saturating_add(active_delta);
                    self.idle = self.idle.saturating_add(idle_delta);
                }
            }
        }
        self.last_tick_at = now;
    }
}

pub fn unix_seconds(value: SystemTime) -> u64 {
    value
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

fn escape_json(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn employee_id_validation_allows_safe_symbols() {
        assert!(EmployeeId::parse("emp-01.test").is_ok());
        assert!(EmployeeId::parse("../bad").is_err());
    }

    #[test]
    fn accumulator_splits_active_and_idle_time() {
        let employee = EmployeeId::parse("emp-1").unwrap();
        let start = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        let mut acc = ActivityAccumulator::new(employee.clone(), Duration::from_secs(5), start);

        acc.record(ActivityEvent {
            employee_id: employee,
            kind: ActivityKind::Keyboard,
            occurred_at: start + Duration::from_secs(1),
        });
        let snapshot = acc.snapshot(start + Duration::from_secs(4));
        assert_eq!(snapshot.active_seconds, 3);
        assert_eq!(snapshot.idle_seconds, 1);
        assert_eq!(snapshot.total_events, 1);

        let snapshot = acc.snapshot(start + Duration::from_secs(8));
        assert_eq!(snapshot.active_seconds, 5);
        assert_eq!(snapshot.idle_seconds, 3);
    }
}
