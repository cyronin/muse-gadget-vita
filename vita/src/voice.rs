//! Muse's reply, spoken. One thread fetches the speech from Azure AI Speech
//! or OpenAI, and a second plays it, so playing starts while the rest is
//! still arriving.

use crate::{azure, clock, files, muse};
use muse_link::{secure, speech};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use vitasdk_sys as sys;

const KEY: &str = "state/openai_key.txt";
const VOICE: &str = "alloy";
const AZURE_KEY: &str = "state/azure_key.txt";
const AZURE_REGION: &str = "state/azure_region.txt";
/// Optional: an Azure voice name, such as en-US-AvaNeural.
const AZURE_VOICE: &str = "state/azure_voice.txt";
/// The most of one answer that is kept, in samples.
const ROOM: usize = speech::RATE as usize * 33;
/// Samples handed to the speaker at a time. The port wants a multiple of 64.
const FRAMES: usize = 1024;
/// Enough arrived that playing will not catch up with the download.
const HEAD_START: usize = speech::RATE as usize / 4;

pub const SILENT: u8 = 0;
pub const PREPARING: u8 = 1;
pub const SPEAKING: u8 = 2;
/// The reply could not be spoken. The screen loop clears this.
pub const FAILED: u8 = 3;
pub static STATE: AtomicU8 = AtomicU8::new(SILENT);

static TEXT: Mutex<Option<String>> = Mutex::new(None);

/// The answer's samples. The fetch thread writes past `ARRIVED` and then
/// moves it, and the speaker reads only below it, so the two never touch
/// the same sample and need no lock.
static SAMPLES: AtomicPtr<i16> = AtomicPtr::new(core::ptr::null_mut());
static ARRIVED: AtomicUsize = AtomicUsize::new(0);
static COMPLETE: AtomicBool = AtomicBool::new(false);

/// Speaks `text`. Call only while silent, which the one-question-at-a-time
/// rule gives for free.
pub fn say(text: &str) {
    STATE.store(PREPARING, Ordering::Relaxed);
    *TEXT.lock().unwrap() = Some(String::from(text));
}

/// One word from a file on the card, such as a key.
fn word(path: &str) -> Option<String> {
    files::read(path).and_then(|file| speech::key_from(&file))
}

/// Azure, when its key and region are on the card. It is preferred, as the
/// one with a free allowance.
fn azure_account() -> Option<azure::Account> {
    Some(azure::Account {
        key: word(AZURE_KEY)?,
        region: word(AZURE_REGION)?,
        voice: word(AZURE_VOICE).unwrap_or_else(|| azure::DEFAULT_VOICE.into()),
    })
}

fn fetch(text: &str) -> Result<(), String> {
    let samples = SAMPLES.load(Ordering::Relaxed);
    let mut odd: Option<u8> = None;
    let mut arrived = 0;
    let mut heard = |piece: &[u8]| {
        for &byte in piece {
            match odd.take() {
                None => odd = Some(byte),
                Some(low) if arrived < ROOM => {
                    // A little louder than it comes, since the speaker is small.
                    let loud = i16::from_le_bytes([low, byte]) as i32 * 3 / 2;
                    unsafe { samples.add(arrived).write(loud.clamp(-32_768, 32_767) as i16) };
                    arrived += 1;
                }
                Some(_) => {}
            }
        }
        ARRIVED.store(arrived, Ordering::Release);
    };
    let outcome = if let Some(account) = azure_account() {
        azure::speak(&account, text, &mut heard)
    } else {
        let key = word(KEY).ok_or("no Azure or OpenAI key on the memory card")?;
        let settings = secure::settings(Arc::new(clock::Rtc)).map_err(|error| format!("{:?}", error))?;
        speech::speak(&mut muse::Vita, settings, &key, VOICE, text, &mut heard).map_err(|error| format!("{:?}", error))
    };
    outcome.map_err(|error| format!("{} after {} samples", error, ARRIVED.load(Ordering::Relaxed)))
}

fn fetching() {
    loop {
        let Some(text) = TEXT.lock().unwrap().take() else {
            thread::sleep(Duration::from_millis(20));
            continue;
        };
        let mut timer = crate::report::Timer::start();
        let outcome = fetch(&text);
        crate::say!("speech: {:?}, {} samples in {} ms", outcome, ARRIVED.load(Ordering::Relaxed), timer.lap());
        if ARRIVED.load(Ordering::Relaxed) == 0 {
            STATE.store(FAILED, Ordering::Relaxed);
        }
        COMPLETE.store(true, Ordering::Release);
    }
}

fn speaking() {
    // The background music port takes the speech's own rate, so it plays
    // as it comes.
    let port = unsafe {
        sys::sceAudioOutOpenPort(sys::SCE_AUDIO_OUT_PORT_TYPE_BGM, FRAMES as i32, speech::RATE as i32, sys::SCE_AUDIO_OUT_MODE_MONO)
    };
    crate::say!("speech: speaker port {:#x}", port);
    if port < 0 {
        return;
    }
    let samples = SAMPLES.load(Ordering::Relaxed);
    let mut frames = [0i16; FRAMES];
    // How far into the answer the speaker is.
    let mut at = 0;
    loop {
        // In this order, a `complete` that reads true comes with the final count.
        let complete = COMPLETE.load(Ordering::Acquire);
        let arrived = ARRIVED.load(Ordering::Acquire);
        let started = STATE.load(Ordering::Relaxed) == SPEAKING;
        if at >= arrived || (!started && !complete && arrived < HEAD_START) {
            if complete && STATE.load(Ordering::Relaxed) != FAILED {
                at = 0;
                ARRIVED.store(0, Ordering::Release);
                COMPLETE.store(false, Ordering::Release);
                STATE.store(SILENT, Ordering::Relaxed);
            }
            thread::sleep(Duration::from_millis(10));
            continue;
        }
        STATE.store(SPEAKING, Ordering::Relaxed);
        let count = (arrived - at).min(FRAMES);
        let ready = unsafe { core::slice::from_raw_parts(samples.add(at), count) };
        frames[..count].copy_from_slice(ready);
        frames[count..].fill(0);
        at += count;
        unsafe { sys::sceAudioOutOutput(port, frames.as_ptr() as *const _) };
    }
}

/// Call after `net::start`, like `muse::start`.
pub fn start() {
    let room: &'static mut [i16] = Vec::leak(vec![0i16; ROOM]);
    SAMPLES.store(room.as_mut_ptr(), Ordering::Relaxed);
    for (name, entry, stack) in [("speech", fetching as fn(), 256 * 1024), ("speaker", speaking as fn(), 64 * 1024)] {
        if let Err(error) = thread::Builder::new().name(name.into()).stack_size(stack).spawn(entry) {
            crate::say!("FAIL: no thread for {}: {}", name, error);
        }
    }
}
