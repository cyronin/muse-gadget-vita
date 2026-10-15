mod audio;
mod avatar;
mod clock;
mod files;
mod input;
mod muse;
mod net;
mod report;
mod screen;
mod status;
mod store;
mod voice;

use avatar::{Avatar, Mood};
use report::Timer;
use screen::Screen;
use status::Link;
use std::sync::atomic::Ordering;

/// How long Muse can be out of reach before the screen says so. A Muse that
/// has been idle can take a while to wake, and the app keeps trying.
const PATIENCE_MS: u32 = 45_000;
/// How long the avatar shows that an answer came when it cannot be spoken.
const UNSPOKEN_MS: u32 = 3000;

fn main() {
    files::prepare();
    report::begin();
    say!("Muse {} for the PS Vita, in {}", env!("CARGO_PKG_VERSION"), files::HOME);
    say!("clock: {:?} s since 1970", clock::unix_seconds());
    let Some(avatar) = files::read("avatar.bin").and_then(Avatar::parse) else {
        say!("FAIL: avatar.bin is missing or not one tools/avatar.sh wrote");
        return;
    };
    match net::start() {
        Ok(()) => muse::start(),
        Err(why) => {
            say!("FAIL: {}", why);
            muse::LINK.store(muse::DOWN, Ordering::Relaxed);
        }
    }
    audio::start();
    input::start();

    let mut screen = Screen::open();
    let mut mood = Mood::Boot;
    let mut since = Timer::start();
    let mut shown_ms = 0;
    loop {
        let now_ms = report::now_ms();
        let link = match muse::LINK.load(Ordering::Relaxed) {
            muse::CONNECTED => Link::Online,
            muse::DOWN => Link::Offline,
            muse::TROUBLE if now_ms.wrapping_sub(muse::TROUBLE_SINCE_MS.load(Ordering::Relaxed)) > PATIENCE_MS => Link::Offline,
            _ => Link::Connecting,
        };
        let mut turn = muse::TURN.load(Ordering::Relaxed);
        let may_talk = link == Link::Online && turn == muse::NO_QUESTION;
        audio::TALKING.store(may_talk && input::talking(), Ordering::Relaxed);
        let speech = voice::STATE.load(Ordering::Relaxed);
        if turn == muse::ANSWERED {
            // The turn ends when the reply has been spoken. If it could not
            // be, the avatar celebrates the answer in silence for a moment.
            let unspoken_shown = speech == voice::FAILED && mood == Mood::Happy && shown_ms >= UNSPOKEN_MS;
            if unspoken_shown {
                voice::STATE.store(voice::SILENT, Ordering::Relaxed);
            }
            if unspoken_shown || speech == voice::SILENT {
                turn = muse::NO_QUESTION;
                muse::TURN.store(turn, Ordering::Relaxed);
            }
        }
        let dancing = (muse::DANCE_UNTIL_MS.load(Ordering::Relaxed).wrapping_sub(now_ms) as i32) > 0;
        let next = match (audio::RECORDING.load(Ordering::Relaxed), speech, turn, link) {
            (true, _, _, _) => Mood::Listening,
            (_, voice::SPEAKING, _, _) => Mood::Speaking,
            (_, voice::FAILED, muse::ANSWERED, _) => Mood::Happy,
            (_, _, muse::ASKED | muse::ANSWERED, _) => Mood::Thinking,
            (_, _, _, Link::Online) if dancing => Mood::Happy,
            (_, _, _, Link::Connecting) => Mood::Boot,
            (_, _, _, Link::Online) => Mood::Idle,
            (_, _, _, Link::Offline) => Mood::Error,
        };
        if next != mood {
            say!("mood {:?}", next);
            mood = next;
            shown_ms = 0;
            since.lap();
        }
        shown_ms += since.lap();
        avatar.draw(&mut screen, mood, shown_ms);
        status::draw(&mut screen, link, now_ms);
        screen.next_frame();
    }
}
