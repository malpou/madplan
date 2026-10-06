//! Local structured telemetry: one JSON object per line in logs/telemetry.jsonl.
//! Every line carries `ts` (RFC 3339 UTC), `run` (one id per app launch) and `kind`.
//! Nothing leaves the machine; the file exists so failures can be debugged after the fact:
//!   jq -c 'select(.level == "error")' logs/telemetry.jsonl

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value, json};

const MAX_BYTES: u64 = 20 * 1024 * 1024;

static LOG: OnceLock<(Mutex<File>, String)> = OnceLock::new();

/// Open (and rotate past MAX_BYTES) logs/telemetry.jsonl and log panics.
pub fn init(root: &Path) {
    let dir = root.join("logs");
    let path = dir.join("telemetry.jsonl");
    let _ = std::fs::create_dir_all(&dir);
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = std::fs::rename(&path, dir.join("telemetry.1.jsonl"));
    }
    let Ok(file) = OpenOptions::new().create(true).append(true).open(&path) else {
        eprintln!("telemetry: couldn't open {}", path.display());
        return;
    };
    let run = format!("{:x}-{}", now_ms(), std::process::id());
    let _ = LOG.set((Mutex::new(file), run));

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        error("panic", json!({
            "message": info.to_string(),
            "backtrace": std::backtrace::Backtrace::force_capture().to_string(),
        }));
        default_hook(info);
    }));
}

pub fn info(kind: &str, data: Value) {
    write("info", kind, data);
}

pub fn error(kind: &str, data: Value) {
    write("error", kind, data);
}

fn write(level: &str, kind: &str, data: Value) {
    let Some((file, run)) = LOG.get() else { return };
    let mut line = Map::new();
    line.insert("ts".into(), rfc3339(now_ms()).into());
    line.insert("run".into(), run.clone().into());
    line.insert("level".into(), level.into());
    line.insert("kind".into(), kind.into());
    match data {
        Value::Object(m) => line.extend(m),
        Value::Null => {}
        other => {
            line.insert("data".into(), other);
        }
    }
    if let Ok(mut f) = file.lock() {
        let _ = writeln!(f, "{}", Value::Object(line));
    }
}

pub fn now_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}

/// Unix ms → "YYYY-MM-DDTHH:MM:SS.mmmZ" (civil-from-days, Howard Hinnant).
fn rfc3339(ms: u128) -> String {
    let secs = (ms / 1000) as i64;
    let (days, sod) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z", sod / 3600, sod % 3600 / 60, sod % 60, ms % 1000)
}

#[cfg(test)]
mod tests {
    #[test]
    fn formats_timestamps() {
        assert_eq!(super::rfc3339(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(super::rfc3339(1_791_308_595_495), "2026-10-06T17:43:15.495Z");
        assert_eq!(super::rfc3339(951_782_400_000), "2000-02-29T00:00:00.000Z");
    }
}
