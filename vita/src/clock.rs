//! The date, from the Vita's real-time clock.

use std::time::Duration;
use rustls::pki_types::UnixTime;
use rustls::time_provider::TimeProvider;
use vitasdk_sys as sys;

const RTC_TICKS_AT_UNIX_EPOCH: u64 = 62_135_596_800_000_000;
const RTC_TICKS_PER_SECOND: u64 = 1_000_000;

/// Seconds since 1970, or `None` when the clock was never set.
pub fn unix_seconds() -> Option<u64> {
    let mut tick = sys::SceRtcTick { tick: 0 };
    if unsafe { sys::sceRtcGetCurrentTick(&mut tick) } != 0 {
        return None;
    }
    Some(tick.tick.checked_sub(RTC_TICKS_AT_UNIX_EPOCH)? / RTC_TICKS_PER_SECOND)
}

#[derive(Debug)]
pub struct Rtc;

impl TimeProvider for Rtc {
    fn current_time(&self) -> Option<UnixTime> {
        unix_seconds().map(|seconds| UnixTime::since_unix_epoch(Duration::from_secs(seconds)))
    }
}
