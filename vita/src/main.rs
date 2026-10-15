mod avatar;
mod clock;
mod files;
mod input;
mod report;
mod screen;
mod status;
mod store;

use avatar::{Avatar, Mood};
use report::Timer;
use screen::Screen;
use status::Link;

fn main() {
    files::prepare();
    report::begin();
    say!("Muse {} for the PS Vita, in {}", env!("CARGO_PKG_VERSION"), files::HOME);
    say!("clock: {:?} s since 1970", clock::unix_seconds());
    let Some(avatar) = files::read("avatar.bin").and_then(Avatar::parse) else {
        say!("FAIL: avatar.bin is missing or not one tools/avatar.sh wrote");
        return;
    };
    match store::load() {
        Ok(saved) => say!("paired as {}", saved.node_id),
        Err(why) => say!("not paired: {}", why),
    }
    input::start();
    let mut screen = Screen::open();
    let mut mood = Mood::Boot;
    let mut since = Timer::start();
    let mut shown_ms = 0;
    loop {
        // Until the microphone is ported, holding the talk button only shows it.
        let next = if input::talking() { Mood::Listening } else { Mood::Boot };
        if next != mood {
            say!("mood {:?}", next);
            mood = next;
            shown_ms = 0;
            since.lap();
        }
        shown_ms += since.lap();
        avatar.draw(&mut screen, mood, shown_ms);
        status::draw(&mut screen, Link::Connecting, report::now_ms());
        screen.next_frame();
    }
}
