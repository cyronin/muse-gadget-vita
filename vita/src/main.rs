mod clock;
mod files;
mod report;

use vitasdk_sys as sys;

fn main() {
    files::prepare();
    report::begin();
    say!("Muse {} for the PS Vita, in {}", env!("CARGO_PKG_VERSION"), files::HOME);
    say!("clock: {:?} s since 1970", clock::unix_seconds());
    loop {
        unsafe { sys::sceDisplayWaitVblankStart() };
    }
}
