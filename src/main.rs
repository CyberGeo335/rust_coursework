use std::env;
use std::thread;
use std::time::Duration;

use employee_time_tracker::{ActivityKind, ActivityTracker, TrackerConfig};

fn main() {
    let employee_id = env::args()
        .nth(1)
        .unwrap_or_else(|| "demo-employee".to_owned());
    let endpoint = env::args()
        .nth(2)
        .unwrap_or_else(|| "http://127.0.0.1:8080/reports".to_owned());

    let mut config = TrackerConfig::new(employee_id, endpoint);
    config.report_interval = Duration::from_secs(10);

    let tracker = match ActivityTracker::start(config) {
        Ok(tracker) => tracker,
        Err(error) => {
            eprintln!("failed to start tracker: {error}");
            std::process::exit(1);
        }
    };

    for _ in 0..3 {
        if let Err(error) = tracker.record_activity(ActivityKind::Heartbeat) {
            eprintln!("failed to record activity: {error}");
            break;
        }
        thread::sleep(Duration::from_secs(1));
    }

    match tracker.flush() {
        Ok(snapshot) => println!("{}", snapshot.to_json()),
        Err(error) => eprintln!("failed to flush tracker: {error}"),
    }
    tracker.stop();
}
