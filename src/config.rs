use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct TrackerConfig {
    pub employee_id: String,
    pub report_endpoint: String,
    pub idle_after: Duration,
    pub report_interval: Duration,
    pub retry_attempts: usize,
    pub retry_backoff: Duration,
    pub spool_dir: PathBuf,
    pub log_file: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackerConfigError {
    EmptyEmployeeId,
    InvalidEndpoint,
    InvalidIdleTimeout,
    InvalidReportInterval,
    EmptySpoolDir,
    EmptyLogFile,
}

impl TrackerConfig {
    pub fn new(employee_id: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self {
            employee_id: employee_id.into(),
            report_endpoint: endpoint.into(),
            idle_after: Duration::from_secs(300),
            report_interval: Duration::from_secs(60),
            retry_attempts: 3,
            retry_backoff: Duration::from_millis(250),
            spool_dir: PathBuf::from("tracker-spool"),
            log_file: PathBuf::from("tracker.log"),
        }
    }

    pub fn validate(&self) -> Result<(), TrackerConfigError> {
        if self.employee_id.trim().is_empty() {
            return Err(TrackerConfigError::EmptyEmployeeId);
        }
        if !self.report_endpoint.starts_with("http://") {
            return Err(TrackerConfigError::InvalidEndpoint);
        }
        if self.idle_after < Duration::from_secs(5) {
            return Err(TrackerConfigError::InvalidIdleTimeout);
        }
        if self.report_interval < Duration::from_secs(1) {
            return Err(TrackerConfigError::InvalidReportInterval);
        }
        if self.spool_dir.as_os_str().is_empty() {
            return Err(TrackerConfigError::EmptySpoolDir);
        }
        if self.log_file.as_os_str().is_empty() {
            return Err(TrackerConfigError::EmptyLogFile);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_employee_id() {
        let cfg = TrackerConfig::new(" ", "http://localhost:8080/report");
        assert_eq!(cfg.validate(), Err(TrackerConfigError::EmptyEmployeeId));
    }

    #[test]
    fn rejects_non_http_endpoint() {
        let cfg = TrackerConfig::new("emp-1", "file:///tmp/report");
        assert_eq!(cfg.validate(), Err(TrackerConfigError::InvalidEndpoint));
    }
}
