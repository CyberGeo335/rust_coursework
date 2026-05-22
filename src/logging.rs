use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::Mutex;
use std::time::SystemTime;

use crate::core::unix_seconds;

pub trait Logger: Send + Sync {
    fn info(&self, message: &str);
    fn warn(&self, message: &str);
    fn error(&self, message: &str);
}

pub struct FileLogger {
    file: Mutex<File>,
}

impl FileLogger {
    pub fn open(path: &Path) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self {
            file: Mutex::new(file),
        })
    }

    fn write(&self, level: &str, message: &str) {
        if let Ok(mut file) = self.file.lock() {
            let _ = writeln!(
                file,
                "{} [{}] {}",
                unix_seconds(SystemTime::now()),
                level,
                message.replace('\n', " ")
            );
        }
    }
}

impl Logger for FileLogger {
    fn info(&self, message: &str) {
        self.write("INFO", message);
    }

    fn warn(&self, message: &str) {
        self.write("WARN", message);
    }

    fn error(&self, message: &str) {
        self.write("ERROR", message);
    }
}

pub struct StdoutLogger;

impl Logger for StdoutLogger {
    fn info(&self, message: &str) {
        println!("INFO {message}");
    }

    fn warn(&self, message: &str) {
        eprintln!("WARN {message}");
    }

    fn error(&self, message: &str) {
        eprintln!("ERROR {message}");
    }
}
