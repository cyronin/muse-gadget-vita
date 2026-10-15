mod avatar;
mod clock;
mod files;
mod report;
mod screen;
mod status;

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
    let mut screen = Screen::open();
    let mood = Mood::Boot;
    let mut since = Timer::start();
    let mut shown_ms = 0;
    loop {
        shown_ms += since.lap();
        avatar.draw(&mut screen, mood, shown_ms);
        status::draw(&mut screen, Link::Connecting, report::now_ms());
        screen.next_frame();
    }
}
