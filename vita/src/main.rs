mod audio;
mod avatar;
mod azure;
mod clock;
mod files;
mod font;
mod input;
mod muse;
mod net;
mod reply;
mod report;
mod screen;
mod status;
mod store;
mod voice;

use avatar::{Avatar, Mood, Place};
use report::Timer;
use screen::Screen;
use status::Link;
use std::sync::atomic::Ordering;

/// How long Muse can be out of reach before the screen says so. A Muse that
/// has been idle can take a while to wake, and the app keeps trying.
const PATIENCE_MS: u32 = 45_000;
/// How long the avatar shows that an answer came when it cannot be spoken.
const UNSPOKEN_MS: u32 = 3000;
/// How long the talk button must stay up before the take ends. A resting
/// finger drops out of the touchscreen's readings for a moment now and then,
/// and one such moment cut a question short on a real Vita.
const LET_GO_MS: u32 = 250;
/// How long the note that the text was turned on or off stays up.
const NOTE_MS: u32 = 1500;
const NOTE_COLOUR: u32 = screen::colour(150, 150, 160);

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
        Ok(()) => {
            muse::start();
            voice::start();
        }
        Err(why) => {
            say!("FAIL: {}", why);
            muse::LINK.store(muse::DOWN, Ordering::Relaxed);
        }
    }
    audio::start();
    input::start();

    let mut screen = Screen::open();
    let mut panel = reply::Panel::new();
    // Without a voice, text is the only way to get the answer.
    let mut show_text = reply::wanted().unwrap_or(!voice::can_speak());
    say!("text {} at start", if show_text { "on" } else { "off" });
    let mut text_was_held = false;
    // The note's words and when it goes, while one is up.
    let mut note: Option<(&[u8], u32)> = None;
    let mut place = avatar.centred();
    // Half size, centred in the room left of the text.
    let beside_text = {
        let size = avatar.side() * avatar::BLOCK / 2;
        Place { left: (reply::LEFT - size) / 2, top: (screen::HEIGHT - size) / 2, block: avatar::BLOCK / 2 }
    };
    let mut mood = Mood::Boot;
    let mut talk_seen_ms: Option<u32> = None;
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
        let held = input::held();
        if held.talk {
            talk_seen_ms = Some(now_ms);
        }
        let holding = talk_seen_ms.is_some_and(|seen_ms| now_ms.wrapping_sub(seen_ms) < LET_GO_MS);
        let talking = may_talk && holding;
        audio::TALKING.store(talking, Ordering::Relaxed);
        // A new question, or Circle, puts the last exchange away.
        if talking || held.dismiss {
            reply::clear();
        }
        let mut speech = voice::STATE.load(Ordering::Relaxed);
        if speech == voice::FAILED && turn == muse::NO_QUESTION {
            // Something Muse said by itself could not be spoken. It is on
            // the screen, and the voice is free for the next.
            voice::STATE.store(voice::SILENT, Ordering::Relaxed);
            speech = voice::SILENT;
        }
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
        // Triangle turns the text on or off, once per press.
        if held.text && !text_was_held {
            show_text = !show_text;
            reply::set_wanted(show_text);
            say!("text {}", if show_text { "on" } else { "off" });
            note = Some((if show_text { b"text on" } else { b"text off" }, now_ms.wrapping_add(NOTE_MS)));
            screen.wipe();
        }
        text_was_held = held.text;
        if note.is_some_and(|(_, until_ms)| (until_ms.wrapping_sub(now_ms) as i32) <= 0) {
            note = None;
            screen.wipe();
        }
        // Kept up to date while hidden, so turning it on shows the last reply.
        let has_text = panel.update() && show_text;
        let wanted = if has_text { beside_text } else { avatar.centred() };
        if wanted != place {
            place = wanted;
            screen.wipe();
        }
        screen.begin();
        avatar.draw(&mut screen, mood, shown_ms, place);
        if has_text {
            panel.scroll(held.up, held.down, now_ms);
            panel.draw(&mut screen);
        }
        status::draw(&mut screen, link, now_ms);
        if let Some((words, _)) = note {
            font::draw(&mut screen, screen::WIDTH - 20 - words.len() * font::CELL, 20, words, NOTE_COLOUR);
        }
        screen.next_frame();
    }
}
