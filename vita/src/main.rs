use vitasdk_sys as sys;

fn main() {
    loop {
        unsafe { sys::sceDisplayWaitVblankStart() };
    }
}
