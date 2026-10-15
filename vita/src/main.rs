mod clock;
mod files;
mod report;
mod screen;

fn main() {
    files::prepare();
    report::begin();
    say!("Muse {} for the PS Vita, in {}", env!("CARGO_PKG_VERSION"), files::HOME);
    say!("clock: {:?} s since 1970", clock::unix_seconds());
    let mut screen = screen::Screen::open();
    loop {
        screen.next_frame();
    }
}
