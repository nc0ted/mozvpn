use anyhow::{Context, Result};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_LOG_SIZE_BYTES: u64 = 2 * 1024 * 1024;

static LOGGER: OnceLock<FileLogger> = OnceLock::new();

pub struct FileLogger {
    log_path: PathBuf,
    backup_path: PathBuf,
    file: Mutex<Option<fs::File>>,
}

impl FileLogger {
    fn new(log_path: PathBuf, backup_path: PathBuf) -> Self {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .ok();
        Self {
            log_path,
            backup_path,
            file: Mutex::new(file),
        }
    }

    fn write_entry(&self, level: &str, message: &str) {
        let timestamp = current_timestamp();
        let formatted = format!("[{}] [{}] {}\n", timestamp, level, message);

        print!("{}", formatted);

        let mut guard = match self.file.lock() {
            Ok(g) => g,
            Err(_) => return,
        };

        if let Ok(metadata) = fs::metadata(&self.log_path) {
            if metadata.len() >= MAX_LOG_SIZE_BYTES {
                *guard = None;
                let _ = fs::remove_file(&self.backup_path);
                let _ = fs::rename(&self.log_path, &self.backup_path);
                *guard = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&self.log_path)
                    .ok();
            }
        }

        if let Some(file) = guard.as_mut() {
            let _ = file.write_all(formatted.as_bytes());
            let _ = file.flush();
        }
    }
}

pub fn init() -> Result<PathBuf> {
    let base_dir = dirs::data_local_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    let log_dir = base_dir.join("outfox");
    fs::create_dir_all(&log_dir).with_context(|| format!("failed to create log dir: {}", log_dir.display()))?;

    let log_path = log_dir.join("outfox.log");
    let backup_path = log_dir.join("outfox.log.old");

    let logger = FileLogger::new(log_path.clone(), backup_path);
    let _ = LOGGER.set(logger);

    log_raw("INFO", &format!("Log initialized at {}", log_path.display()));
    Ok(log_path)
}

pub fn log_raw(level: &str, message: &str) {
    if let Some(logger) = LOGGER.get() {
        logger.write_entry(level, message);
    } else {
        println!("[{}] [{}] {}", current_timestamp(), level, message);
    }
}

fn current_timestamp() -> String {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let total_secs = duration.as_secs();
    let secs_of_day = total_secs % 86400;
    let hours = secs_of_day / 3600;
    let minutes = (secs_of_day % 3600) / 60;
    let seconds = secs_of_day % 60;

    let days = (total_secs / 86400) as i64;
    let (year, month, day) = days_to_ymd(days);

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        year, month, day, hours, minutes, seconds
    )
}

fn days_to_ymd(days: i64) -> (i64, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1024 + doe / 1461 - doe / 142375) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        $crate::logger::log_raw("INFO", &format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        $crate::logger::log_raw("WARN", &format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        $crate::logger::log_raw("ERROR", &format!($($arg)*))
    };
}
