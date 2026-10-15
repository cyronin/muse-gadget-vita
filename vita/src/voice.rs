//! Muse's reply, spoken.

use std::sync::atomic::{AtomicU8, Ordering};

pub const SILENT: u8 = 0;
pub const PREPARING: u8 = 1;
pub const SPEAKING: u8 = 2;
/// The reply could not be spoken. The screen loop clears this.
pub const FAILED: u8 = 3;
pub static STATE: AtomicU8 = AtomicU8::new(SILENT);

/// Speaks `text`. Until the speaker is ported, nothing can be spoken, so the
/// avatar shows that an answer came.
pub fn say(text: &str) {
    crate::say!("speech: no speaker yet for {} characters", text.len());
    STATE.store(FAILED, Ordering::Relaxed);
}
