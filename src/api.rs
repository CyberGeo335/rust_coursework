use std::io;
use std::sync::mpsc;
use std::time::{Duration, SystemTime};

use crate::config::TrackerConfig;
use crate::core::{ActivityAccumulator, ActivityEvent, ActivityKind, ActivitySnapshot, EmployeeId};
use crate::logging::{FileLogger, Logger};
use crate::reporting::HttpReportSink;
use crate::tracker::{TrackerCommand, TrackerWorker};

pub struct ActivityTracker;

impl ActivityTracker {
    pub fn start(config: TrackerConfig) -> io::Result<TrackerHandle> {
        config
            .validate()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, format!("{error:?}")))?;
        let employee_id = EmployeeId::parse(config.employee_id.clone())
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        let logger: Box<dyn Logger> = Box::new(FileLogger::open(&config.log_file)?);
        let sink = Box::new(HttpReportSink::new(
            &config.report_endpoint,
            config.retry_attempts,
            config.retry_backoff,
            config.spool_dir,
        )?);
        let accumulator =
            ActivityAccumulator::new(employee_id.clone(), config.idle_after, SystemTime::now());
        let worker = TrackerWorker::start(accumulator, config.report_interval, sink, logger);
        Ok(TrackerHandle {
            employee_id,
            sender: worker.sender(),
            worker: Some(worker),
        })
    }
}

pub struct TrackerHandle {
    employee_id: EmployeeId,
    sender: mpsc::Sender<TrackerCommand>,
    worker: Option<TrackerWorker>,
}

impl TrackerHandle {
    pub fn record_activity(&self, kind: ActivityKind) -> io::Result<()> {
        let event = ActivityEvent::new(self.employee_id.clone(), kind);
        self.sender
            .send(TrackerCommand::Activity(event))
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "tracker is stopped"))
    }

    pub fn flush(&self) -> io::Result<ActivitySnapshot> {
        let (tx, rx) = mpsc::channel();
        self.sender
            .send(TrackerCommand::Flush(tx))
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "tracker is stopped"))?;
        rx.recv_timeout(Duration::from_secs(5))
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "tracker flush timed out"))
    }

    pub fn stop(mut self) {
        if let Some(worker) = self.worker.take() {
            worker.stop();
        }
    }
}
