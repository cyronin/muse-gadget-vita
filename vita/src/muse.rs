//! The connection to Muse, on a thread of its own. Reaching Muse takes
//! seconds at a time, and the avatar must keep moving meanwhile.

use crate::{clock, net, store, voice};
use muse_link::{secure, App, Client, Clock, Config, Event, Lost, Net, Poll, Security, State};
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use vitasdk_sys as sys;

/// The TLS handshake and the Noise session run deep on this thread's stack.
const STACK: usize = 256 * 1024;

pub const CONNECTING: u8 = 0;
pub const CONNECTED: u8 = 1;
/// The last try failed and another is coming.
pub const TROUBLE: u8 = 2;
/// Nothing more will be tried: there is no pairing, or Muse removed it.
pub const DOWN: u8 = 3;
pub static LINK: AtomicU8 = AtomicU8::new(CONNECTING);
/// When the trouble began, on `report::now_ms`'s clock. One slow start is
/// not worth alarming anyone over, so the screen waits before calling it.
pub static TROUBLE_SINCE_MS: AtomicU32 = AtomicU32::new(0);

fn trouble() {
    if LINK.swap(TROUBLE, Ordering::Relaxed) != TROUBLE {
        TROUBLE_SINCE_MS.store(crate::report::now_ms(), Ordering::Relaxed);
    }
}

/// When the dance Muse asked for ends, on `report::now_ms`'s clock.
pub static DANCE_UNTIL_MS: AtomicU32 = AtomicU32::new(0);

/// What Muse can ask this Vita to do, in the form it calls commands_v2.
const COMMANDS: &str = r#"{
 "vita.say": {
  "description": "Speak a short message aloud through the speaker of the user's PS Vita. Keep it under about 200 characters.",
  "required": {"text": {"type": "string", "description": "The message to speak."}},
  "optional": {}
 },
 "vita.dance": {
  "description": "Make the Muse avatar on the user's Vita dance. The avatar is you, so call this when the user says 'make Muse dance', 'dance', or asks you or their Vita to dance.",
  "required": {},
  "optional": {"seconds": {"type": "number", "description": "How long, 2 to 60. Default 8."}}
 },
 "vita.status": {
  "description": "Read the user's PS Vita's battery level, whether it is charging or plugged in, and its Wi-Fi network and signal strength. Call this when the user asks about their Vita's battery, power or connection.",
  "required": {},
  "optional": {}
 }
}"#;

pub const NO_QUESTION: u8 = 0;
pub const ASKED: u8 = 1;
pub const ANSWERED: u8 = 2;
pub static TURN: AtomicU8 = AtomicU8::new(NO_QUESTION);

/// A recording on its way from the audio thread.
static TAKE: Mutex<Option<Vec<u8>>> = Mutex::new(None);

/// Hands a WAV recording to Muse as the next question.
#[allow(dead_code)]
pub fn ask(wav: Vec<u8>) {
    TURN.store(ASKED, Ordering::Relaxed);
    *TAKE.lock().unwrap() = Some(wav);
}

fn taken() -> Option<Vec<u8>> {
    TAKE.lock().unwrap().take()
}

pub struct Vita;

impl Net for Vita {
    type Wire = net::Socket;

    fn open(&mut self, host: &str, port: u16) -> Result<net::Socket, Lost> {
        net::lookup(host).and_then(|address| net::Socket::connect(address, port)).map_err(|why| {
            crate::say!("net: {}", why);
            Lost
        })
    }
}

impl Clock for Vita {
    fn millis(&self) -> u64 {
        crate::report::micros() / 1000
    }

    fn unix_seconds(&self) -> Option<u64> {
        clock::unix_seconds()
    }
}

/// What the client tells the app, turned into what the avatar and the
/// voice do.
#[derive(Default)]
struct Avatar {
    /// Whether this question's reply has been sent to be spoken.
    spoken: bool,
    /// The recording, until Muse says it heard it. On the PSP an upload
    /// stalled often enough that one lost on the way is asked again without
    /// the user having to.
    unheard: Option<Vec<u8>>,
    again: Again,
}

#[derive(Default, PartialEq)]
enum Again {
    #[default]
    Allowed,
    /// To be asked when Muse is next reachable, unless that takes too long.
    Wanted { since_ms: u32 },
    Used,
}

/// How long a question waits for Muse to come back before it is dropped.
const AGAIN_WITHIN_MS: u32 = 60_000;

impl Avatar {
    fn speak(&mut self, text: &str) {
        if !core::mem::replace(&mut self.spoken, true) && !text.trim().is_empty() {
            voice::say(text);
        }
    }
}

impl App for Avatar {
    fn event(&mut self, event: Event<'_>) {
        match event {
            Event::State(state) => {
                crate::say!("muse: {:?}", state);
                match state {
                    State::Connected => LINK.store(CONNECTED, Ordering::Relaxed),
                    // A drop is followed by a new try at once, which says
                    // nothing yet about whether Muse can be reached.
                    State::Connecting if LINK.load(Ordering::Relaxed) != TROUBLE => LINK.store(CONNECTING, Ordering::Relaxed),
                    State::Connecting => {}
                    State::Unreachable(_) => trouble(),
                    State::Unpaired => LINK.store(DOWN, Ordering::Relaxed),
                }
            }
            Event::Log(line) => crate::say!("muse: {}", line),
            Event::Heard(text) => {
                crate::say!("heard: {}", text);
                self.unheard = None;
            }
            Event::Reply(_) => {}
            // The stream has paused, which is almost always the whole reply,
            // a couple of seconds before `Done` confirms it.
            Event::Settled(text) => {
                crate::say!("settled: {} characters", text.len());
                self.speak(text);
            }
            Event::Done(text) => {
                crate::say!("reply: {}", text);
                self.speak(text);
                self.spoken = false;
                self.unheard = None;
                TURN.store(ANSWERED, Ordering::Relaxed);
            }
            Event::Failed { why, .. } => {
                self.spoken = false;
                if let Some((_, signal)) = net::wifi() {
                    crate::say!("signal {} of 100 when the question failed", signal);
                }
                if self.unheard.is_some() && self.again == Again::Allowed {
                    crate::say!("question failed: {}, asking again", why);
                    self.again = Again::Wanted { since_ms: crate::report::now_ms() };
                } else {
                    crate::say!("question failed: {}", why);
                    self.unheard = None;
                    TURN.store(NO_QUESTION, Ordering::Relaxed);
                }
            }
        }
    }

    fn invoke(&mut self, command: &str, params: &str) -> Result<String, String> {
        crate::say!("command: {}", command);
        let params: serde_json::Value = serde_json::from_str(params).map_err(|_| String::from("the parameters are not JSON"))?;
        match command {
            "vita.say" => {
                let text = params["text"].as_str().map(str::trim).filter(|text| !text.is_empty()).ok_or("there is no text to say")?;
                if voice::STATE.load(Ordering::Relaxed) != voice::SILENT {
                    return Err("the Vita is already speaking".into());
                }
                voice::say(text);
                Ok("{}".into())
            }
            "vita.dance" => {
                let seconds = params["seconds"].as_f64().unwrap_or(8.0).clamp(2.0, 60.0) as u32;
                DANCE_UNTIL_MS.store(crate::report::now_ms().wrapping_add(seconds * 1000), Ordering::Relaxed);
                Ok("{}".into())
            }
            "vita.status" => {
                let (percent, charging, plugged) = unsafe {
                    (sys::scePowerGetBatteryLifePercent(), sys::scePowerIsBatteryCharging() != 0, sys::scePowerIsPowerOnline() != 0)
                };
                let wifi = net::wifi();
                Ok(serde_json::json!({
                    "battery_percent": percent,
                    "charging": charging,
                    "plugged_in": plugged,
                    "wifi_network": wifi.as_ref().map(|(name, _)| name.as_str()),
                    "wifi_signal_percent": wifi.as_ref().map(|&(_, strength)| strength),
                })
                .to_string())
            }
            _ => Err(format!("this Vita does not run {}", command)),
        }
    }
}

fn serve() -> Result<(), String> {
    while let Err(why) = net::join() {
        crate::say!("{}, trying again in 5 s", why);
        trouble();
        thread::sleep(Duration::from_secs(5));
    }
    match net::wifi() {
        Some((name, signal)) => crate::say!("Wi-Fi joined: {:?}, signal {} of 100", name, signal),
        None => crate::say!("Wi-Fi joined"),
    }
    let saved = store::load()?;
    let mut config = Config::new(&saved.node_id, "Vita", env!("CARGO_PKG_VERSION"), concat!("muse-gadget-vita/", env!("CARGO_PKG_VERSION")));
    config.commands_json = Some(COMMANDS.into());
    config.sdk_token = saved.sdk_token;
    config.api_root = saved.api_root;
    config.noise_host = saved.noise_host;
    let security = Security::Tls(secure::settings(Arc::new(clock::Rtc)).map_err(|error| format!("{:?}", error))?);
    let mut client = Client::new(config, saved.pairing, security, Vita, Vita, saved.store).map_err(|error| format!("{:?}", error))?;
    let mut app = Avatar::default();
    loop {
        match client.poll(&mut app, 10) {
            Poll::Active => {}
            Poll::Resting(wait_ms) => thread::sleep(Duration::from_millis(wait_ms.min(50) as u64)),
            Poll::Unpaired => return Err("Muse no longer knows this Vita. It has to be paired again.".into()),
        }
        let fresh = taken();
        if fresh.is_some() {
            app.again = Again::Allowed;
        }
        let again = match app.again {
            Again::Wanted { since_ms } if crate::report::now_ms().wrapping_sub(since_ms) > AGAIN_WITHIN_MS => {
                crate::say!("question dropped: Muse did not come back in time");
                app.again = Again::Used;
                app.unheard = None;
                TURN.store(NO_QUESTION, Ordering::Relaxed);
                None
            }
            Again::Wanted { .. } if client.state() == State::Connected && !client.asking() => {
                app.again = Again::Used;
                app.unheard.take()
            }
            _ => None,
        };
        if let Some(wav) = fresh.or(again) {
            app.unheard = Some(wav.clone());
            if let Err(refused) = client.ask_voice(wav) {
                crate::say!("question refused: {:?}", refused);
                app.unheard = None;
                TURN.store(NO_QUESTION, Ordering::Relaxed);
            }
        }
    }
}

/// Starts the connection. Call after `net::start`.
pub fn start() {
    let started = thread::Builder::new().name("muse".into()).stack_size(STACK).spawn(|| {
        if let Err(why) = serve() {
            crate::say!("FAIL: {}", why);
        }
        LINK.store(DOWN, Ordering::Relaxed);
    });
    if let Err(error) = started {
        crate::say!("FAIL: no thread for Muse: {}", error);
        LINK.store(DOWN, Ordering::Relaxed);
    }
}
