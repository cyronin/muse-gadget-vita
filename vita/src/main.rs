mod files;
mod report;

use vitasdk_sys as sys;

fn main() {
    files::prepare();
    report::begin();
    say!("Muse {} for the PS Vita, in {}", env!("CARGO_PKG_VERSION"), files::HOME);
    loop {
        unsafe { sys::sceDisplayWaitVblankStart() };
    }
}
