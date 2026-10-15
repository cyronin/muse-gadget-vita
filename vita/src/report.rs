//! Progress, in a file in the app's folder. The screen belongs to the
//! avatar, so nothing is printed there. The file is written line by line so
//! a stall still leaves everything up to it.

use crate::files;
use std::fmt;
use vitasdk_sys as sys;

const PATH: &str = "log.txt";
const BEFORE: &str = "log-before.txt";

/// Microseconds since the app started.
pub fn micros() -> u64 {
    unsafe { sys::sceKernelGetProcessTimeWide() }
}

/// Milliseconds since the app started.
pub fn now_ms() -> u32 {
    (micros() / 1000) as u32
}

/// Starts a new log and keeps the last one. What went wrong is often in the
/// run before the one that shows it.
pub fn begin() {
    files::remove(BEFORE);
    files::rename(PATH, BEFORE);
    files::write(PATH, b"");
}

pub fn line(text: fmt::Arguments) {
    files::append(PATH, format!("{:>7} {}\n", now_ms(), text).as_bytes());
}

#[macro_export]
macro_rules! say {
    ($($arg:tt)*) => { $crate::report::line(format_args!($($arg)*)) };
}

pub struct Timer(u64);

impl Timer {
    pub fn start() -> Self {
        Timer(micros())
    }

    /// Milliseconds since the last lap.
    pub fn lap(&mut self) -> u32 {
        let now = micros();
        let passed = (now - self.0) / 1000;
        self.0 = now;
        passed as u32
    }
}
