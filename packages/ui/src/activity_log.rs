use dioxus::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub enum LogLevel {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: LogLevel,
    pub message: String,
}

#[derive(Clone, Debug, Default)]
pub struct ActivityLog {
    pub entries: Vec<LogEntry>,
    pub visible: bool,
}

pub fn use_activity_log() -> Signal<ActivityLog> {
    use_context::<Signal<ActivityLog>>()
}

pub fn log_activity(log: &mut Signal<ActivityLog>, level: LogLevel, message: &str) {
    let ts = current_time();
    log.write().entries.push(LogEntry {
        timestamp: ts,
        level,
        message: message.to_string(),
    });
}

#[cfg(target_arch = "wasm32")]
fn current_time() -> String {
    let date = js_sys::Date::new_0();
    let h = date.get_hours();
    let m = date.get_minutes();
    let s = date.get_seconds();
    format!("{h:02}:{m:02}:{s:02}")
}

#[cfg(not(target_arch = "wasm32"))]
fn current_time() -> String {
    "00:00:00".to_string()
}
