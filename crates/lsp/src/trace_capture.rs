//! Runtime trace capture using Chrome trace format.
//!
//! Provides a reloadable tracing layer that can be swapped between a no-op
//! and a `tracing-chrome` layer at runtime via LSP custom requests.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use tracing_chrome::ChromeLayerBuilder;
use tracing_subscriber::Layer;

/// Type-erased boxed layer for the reload handle.
type BoxedLayer = Box<dyn Layer<tracing_subscriber::Registry> + Send + Sync>;

/// Handle to swap the reloadable tracing layer at runtime.
pub type ReloadHandle =
    tracing_subscriber::reload::Handle<BoxedLayer, tracing_subscriber::Registry>;

/// Creates a no-op reload layer and its handle.
///
/// The returned layer starts as a no-op. Use `ReloadHandle::reload` to swap it
/// to a `ChromeLayer` when trace capture is started.
#[must_use]
pub fn create_reload_layer() -> (
    tracing_subscriber::reload::Layer<BoxedLayer, tracing_subscriber::Registry>,
    ReloadHandle,
) {
    let noop: BoxedLayer = Box::new(tracing_subscriber::layer::Identity::new());
    tracing_subscriber::reload::Layer::new(noop)
}

const AUTO_STOP_SECS: u64 = 60;

/// State held while a trace capture is active.
struct ActiveCapture {
    trace_file_path: PathBuf,
    started_at: Instant,
    // The FlushGuard flushes and closes the trace file when dropped.
    // Stored as Option so we can take() it on stop.
    _flush_guard: tracing_chrome::FlushGuard,
}

/// Manages trace capture lifecycle.
pub struct TraceCaptureManager {
    reload_handle: ReloadHandle,
    active: Mutex<Option<ActiveCapture>>,
}

#[derive(Debug, serde::Deserialize)]
pub struct TraceCaptureParams {
    pub action: String,
}

#[derive(Debug, serde::Serialize)]
pub struct TraceCaptureResult {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

impl TraceCaptureManager {
    pub fn new(reload_handle: ReloadHandle) -> Self {
        Self {
            reload_handle,
            active: Mutex::new(None),
        }
    }

    pub fn start(&self) -> TraceCaptureResult {
        let mut active = self.active.lock().unwrap();

        if active.is_some() {
            return TraceCaptureResult {
                status: "error".to_string(),
                path: None,
                message: Some("Trace capture is already running".to_string()),
                duration_ms: None,
            };
        }

        let timestamp = chrono_timestamp();
        let trace_file_path =
            std::env::temp_dir().join(format!("graphql-analyzer-trace-{timestamp}.json"));

        let (chrome_layer, flush_guard) = ChromeLayerBuilder::new()
            .file(trace_file_path.clone())
            .include_args(true)
            .build();

        // Don't use .with_filter() here — per-layer filtering requires
        // registering a FilterId at subscriber build time, which can't
        // happen for a layer created at runtime via reload. The chrome
        // layer captures all spans; users can filter in the trace viewer.
        let layer: BoxedLayer = Box::new(chrome_layer);

        if let Err(e) = self.reload_handle.reload(layer) {
            return TraceCaptureResult {
                status: "error".to_string(),
                path: None,
                message: Some(format!("Failed to enable trace layer: {e}")),
                duration_ms: None,
            };
        }

        *active = Some(ActiveCapture {
            trace_file_path: trace_file_path.clone(),
            started_at: Instant::now(),
            _flush_guard: flush_guard,
        });

        TraceCaptureResult {
            status: "started".to_string(),
            path: Some(trace_file_path.to_string_lossy().to_string()),
            message: Some(format!(
                "Trace capture started. Auto-stops after {AUTO_STOP_SECS}s."
            )),
            duration_ms: None,
        }
    }

    pub fn stop(&self) -> TraceCaptureResult {
        let mut active = self.active.lock().unwrap();

        let Some(capture) = active.take() else {
            return TraceCaptureResult {
                status: "error".to_string(),
                path: None,
                message: Some("No trace capture is running".to_string()),
                duration_ms: None,
            };
        };

        let duration = capture.started_at.elapsed();
        let path = capture.trace_file_path.clone();

        // Swap back to no-op before dropping the guard, so no new spans
        // are written while the file is being flushed.
        let noop: BoxedLayer = Box::new(tracing_subscriber::layer::Identity::new());
        let _ = self.reload_handle.reload(noop);

        // Drop the capture (and its FlushGuard) to flush the trace file.
        drop(capture);

        TraceCaptureResult {
            status: "stopped".to_string(),
            path: Some(path.to_string_lossy().to_string()),
            message: None,
            duration_ms: Some(duration.as_millis() as u64),
        }
    }

    /// Check if capture has exceeded the auto-stop timeout.
    /// Returns true if it was auto-stopped.
    pub fn check_auto_stop(&self) -> bool {
        let active = self.active.lock().unwrap();
        let should_stop = active
            .as_ref()
            .is_some_and(|c| c.started_at.elapsed().as_secs() >= AUTO_STOP_SECS);
        drop(active);

        if should_stop {
            self.stop();
            return true;
        }
        false
    }

    pub fn is_capturing(&self) -> bool {
        self.active.lock().unwrap().is_some()
    }
}

fn chrono_timestamp() -> String {
    use std::time::SystemTime;

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();

    // Format as ISO-8601-ish for filenames (no colons)
    let hours = (secs % 86400) / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;
    let days = secs / 86400;
    // Simple epoch-days-to-date (good enough for filenames)
    let (year, month, day) = epoch_days_to_date(days);

    format!("{year:04}-{month:02}-{day:02}T{hours:02}-{minutes:02}-{seconds:02}")
}

fn epoch_days_to_date(days: u64) -> (u64, u64, u64) {
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}
