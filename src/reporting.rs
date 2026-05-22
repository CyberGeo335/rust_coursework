use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime};

use crate::core::{unix_seconds, ActivitySnapshot};

pub trait ReportSink: Send {
    fn send(&mut self, snapshot: &ActivitySnapshot) -> io::Result<()>;
}

pub struct HttpReportSink {
    endpoint: HttpEndpoint,
    retry_attempts: usize,
    retry_backoff: Duration,
    spool: FileSpool,
}

impl HttpReportSink {
    pub fn new(
        endpoint: &str,
        retry_attempts: usize,
        retry_backoff: Duration,
        spool_dir: PathBuf,
    ) -> io::Result<Self> {
        Ok(Self {
            endpoint: HttpEndpoint::parse(endpoint)?,
            retry_attempts,
            retry_backoff,
            spool: FileSpool::new(spool_dir)?,
        })
    }

    pub fn flush_spool(&mut self) -> io::Result<()> {
        for path in self.spool.pending()? {
            let mut body = String::new();
            OpenOptions::new()
                .read(true)
                .open(&path)?
                .read_to_string(&mut body)?;
            match self.post_with_retry(&body) {
                Ok(()) => fs::remove_file(path)?,
                Err(_) => break,
            }
        }
        Ok(())
    }

    fn post_with_retry(&self, body: &str) -> io::Result<()> {
        let attempts = self.retry_attempts.max(1);
        let mut last_error = None;
        for attempt in 0..attempts {
            match self.endpoint.post(body) {
                Ok(()) => return Ok(()),
                Err(error) => {
                    last_error = Some(error);
                    if attempt + 1 < attempts {
                        thread::sleep(self.retry_backoff);
                    }
                }
            }
        }
        Err(last_error.unwrap_or_else(|| io::Error::new(io::ErrorKind::Other, "send failed")))
    }
}

impl ReportSink for HttpReportSink {
    fn send(&mut self, snapshot: &ActivitySnapshot) -> io::Result<()> {
        self.flush_spool()?;
        let body = snapshot.to_json();
        if let Err(error) = self.post_with_retry(&body) {
            self.spool.store(&body)?;
            return Err(error);
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct HttpEndpoint {
    host: String,
    port: u16,
    path: String,
}

impl HttpEndpoint {
    fn parse(value: &str) -> io::Result<Self> {
        let without_scheme = value.strip_prefix("http://").ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "only http:// endpoints are supported")
        })?;
        let (host_port, path) = without_scheme
            .split_once('/')
            .map(|(host, path)| (host, format!("/{path}")))
            .unwrap_or((without_scheme, "/".to_owned()));
        if host_port.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "endpoint host is empty"));
        }
        let (host, port) = host_port
            .rsplit_once(':')
            .and_then(|(host, port)| port.parse::<u16>().ok().map(|port| (host, port)))
            .unwrap_or((host_port, 80));
        if host.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "endpoint host is empty"));
        }
        Ok(Self {
            host: host.to_owned(),
            port,
            path,
        })
    }

    fn post(&self, body: &str) -> io::Result<()> {
        let mut stream = TcpStream::connect((self.host.as_str(), self.port))?;
        stream.set_write_timeout(Some(Duration::from_secs(3)))?;
        stream.set_read_timeout(Some(Duration::from_secs(3)))?;
        write!(
            stream,
            "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            self.path,
            self.host,
            body.len(),
            body
        )?;

        let mut response = String::new();
        stream.read_to_string(&mut response)?;
        if response.starts_with("HTTP/1.1 2") || response.starts_with("HTTP/1.0 2") {
            Ok(())
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "server returned non-success status"))
        }
    }
}

pub struct FileSpool {
    dir: PathBuf,
}

impl FileSpool {
    pub fn new(dir: PathBuf) -> io::Result<Self> {
        fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    pub fn store(&self, body: &str) -> io::Result<PathBuf> {
        let now = SystemTime::now();
        let nanos = now
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .subsec_nanos();
        let file_name = format!(
            "report-{}-{}-{}.json",
            unix_seconds(now),
            nanos,
            std::process::id()
        );
        let path = self.dir.join(file_name);
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)?;
        file.write_all(body.as_bytes())?;
        Ok(path)
    }

    pub fn pending(&self) -> io::Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let path = entry?.path();
            if is_json_file(&path) {
                files.push(path);
            }
        }
        files.sort();
        Ok(files)
    }
}

fn is_json_file(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value == "json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_parser_accepts_host_port_path() {
        let endpoint = HttpEndpoint::parse("http://localhost:9090/reports").unwrap();
        assert_eq!(endpoint.host, "localhost");
        assert_eq!(endpoint.port, 9090);
        assert_eq!(endpoint.path, "/reports");
    }

    #[test]
    fn endpoint_parser_rejects_https_for_std_only_mvp() {
        assert!(HttpEndpoint::parse("https://example.com/reports").is_err());
    }

    #[test]
    fn file_spool_stores_pending_reports() {
        let dir = std::env::temp_dir().join(format!(
            "employee-time-tracker-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        let spool = FileSpool::new(dir.clone()).unwrap();

        let stored = spool.store("{\"ok\":true}").unwrap();
        let pending = spool.pending().unwrap();

        assert_eq!(pending, vec![stored]);
        let _ = fs::remove_dir_all(dir);
    }
}
