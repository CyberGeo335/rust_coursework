use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};

use crate::core::{ActivityAccumulator, ActivityEvent, ActivitySnapshot};
use crate::logging::Logger;
use crate::reporting::ReportSink;

pub enum TrackerCommand {
    Activity(ActivityEvent),
    Flush(Sender<ActivitySnapshot>),
    Stop,
}

pub struct TrackerWorker {
    sender: Sender<TrackerCommand>,
    handle: Option<JoinHandle<()>>,
}

impl TrackerWorker {
    pub fn start(
        mut accumulator: ActivityAccumulator,
        report_interval: Duration,
        mut sink: Box<dyn ReportSink>,
        logger: Box<dyn Logger>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel();
        let handle = thread::spawn(move || {
            logger.info("tracker worker started");
            loop {
                match receiver.recv_timeout(report_interval) {
                    Ok(TrackerCommand::Activity(event)) => {
                        logger.info(&format!("activity event received: {}", event.kind));
                        accumulator.record(event);
                    }
                    Ok(TrackerCommand::Flush(reply)) => {
                        logger.info("manual report flush requested");
                        let snapshot = accumulator.snapshot(SystemTime::now());
                        let _ = sink.send(&snapshot).map_err(|error| {
                            logger.warn(&format!("report send failed; spooled if possible: {error}"));
                        });
                        let _ = reply.send(snapshot);
                    }
                    Ok(TrackerCommand::Stop) => {
                        let snapshot = accumulator.snapshot(SystemTime::now());
                        if let Err(error) = sink.send(&snapshot) {
                            logger.warn(&format!("final report send failed: {error}"));
                        }
                        logger.info("tracker worker stopped");
                        break;
                    }
                    Err(RecvTimeoutError::Timeout) => {
                        logger.info("periodic report flush started");
                        let snapshot = accumulator.snapshot(SystemTime::now());
                        if let Err(error) = sink.send(&snapshot) {
                            logger.warn(&format!("periodic report send failed: {error}"));
                        }
                    }
                    Err(RecvTimeoutError::Disconnected) => {
                        logger.warn("tracker command channel disconnected");
                        break;
                    }
                }
            }
        });
        Self {
            sender,
            handle: Some(handle),
        }
    }

    pub fn sender(&self) -> Sender<TrackerCommand> {
        self.sender.clone()
    }

    pub fn stop(mut self) {
        let _ = self.sender.send(TrackerCommand::Stop);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for TrackerWorker {
    fn drop(&mut self) {
        let _ = self.sender.send(TrackerCommand::Stop);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
