//! The microphone, on a thread of its own because a capture blocks for as
//! long as the sound lasts.

use crate::muse;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use vitasdk_sys as sys;

/// Samples the voice port hands over at a time.
const GRAIN: usize = 256;
/// What the raw port might take at 48 kHz. It refused 256 on a real Vita as
/// an invalid size, so these are tried in turn.
const RAW_GRAINS: [usize; 3] = [768, 512, 1024];
/// The rate a take is kept at. The voice note keeps one sample in three of
/// it, which leaves 8 kHz, about what the PSP sent.
const RATE: usize = 24_000;
const LONGEST_TAKE: usize = RATE * 12;
/// A shorter take is a slip of the finger.
const SHORTEST_TAKE: usize = RATE * 2 / 5;

/// Set by the screen loop while the talk button is down.
pub static TALKING: AtomicBool = AtomicBool::new(false);
/// Whether a take is being recorded, for the avatar.
pub static RECORDING: AtomicBool = AtomicBool::new(false);

struct Microphone {
    port: i32,
    /// Microphone samples to each kept sample: 2 at 48 kHz, 1 at the voice
    /// port's 16 kHz.
    group: usize,
    rate: usize,
    room: Vec<i16>,
}

impl Microphone {
    /// The raw port at 48 kHz, or failing that the voice port, which only
    /// records at 16 kHz.
    fn open() -> Option<Microphone> {
        let raw = RAW_GRAINS.map(|grain| (sys::SCE_AUDIO_IN_PORT_TYPE_RAW, grain, 48_000, 2));
        let voice = (sys::SCE_AUDIO_IN_PORT_TYPE_VOICE, GRAIN, 16_000, 1);
        for (kind, grain, rate, group) in raw.into_iter().chain([voice]) {
            let port = unsafe { sys::sceAudioInOpenPort(kind, grain as i32, rate, sys::SCE_AUDIO_IN_PARAM_FORMAT_S16_MONO) };
            crate::say!("audio: mic port {} at {} Hz, {} at a time: {:#x}", kind, rate, grain, port);
            if port >= 0 {
                return Some(Microphone { port, group, rate: rate as usize / group, room: vec![0; grain] });
            }
        }
        None
    }

    /// Waits for the next samples, kept at `self.rate`, and adds them to `take`.
    fn hear(&mut self, take: &mut Vec<i16>) {
        unsafe { sys::sceAudioInInput(self.port, self.room.as_mut_ptr() as *mut _) };
        // The average is what stops the higher pitches folding down into noise.
        take.extend(self.room.chunks_exact(self.group).map(|group| {
            (group.iter().map(|&sample| sample as i32).sum::<i32>() / self.group as i32) as i16
        }));
    }
}

fn run() {
    let Some(mut microphone) = Microphone::open() else {
        crate::say!("FAIL: no microphone");
        return;
    };
    let rate = microphone.rate;
    let (longest, shortest) = (LONGEST_TAKE * rate / RATE, SHORTEST_TAKE * rate / RATE);
    let grain = microphone.room.len() / microphone.group;
    let mut take: Vec<i16> = Vec::with_capacity(longest + grain);
    loop {
        // The microphone keeps running between takes, so a take starts warm.
        take.clear();
        microphone.hear(&mut take);
        if !TALKING.load(Ordering::Relaxed) {
            continue;
        }
        RECORDING.store(true, Ordering::Relaxed);
        while TALKING.load(Ordering::Relaxed) && take.len() + grain <= longest {
            microphone.hear(&mut take);
        }
        let loudest = take.iter().map(|sample| sample.unsigned_abs()).max().unwrap_or(0);
        crate::say!("audio: took {} ms, loudest {} of 32768", take.len() * 1000 / rate, loudest);
        if take.len() >= shortest {
            muse::ask(muse_link::wav::voice_note(&take, rate as u32));
        }
        RECORDING.store(false, Ordering::Relaxed);
    }
}

pub fn start() {
    if let Err(error) = thread::Builder::new().name("audio".into()).stack_size(64 * 1024).spawn(run) {
        crate::say!("FAIL: no thread for the microphone: {}", error);
    }
}
