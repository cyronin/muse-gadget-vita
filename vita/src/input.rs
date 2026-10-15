//! The buttons: the right shoulder button or a finger on the front
//! touchscreen to talk, Triangle to show or hide replies as text, Up and
//! Down to scroll one, Circle to put it away, and Select for the chat menu,
//! where Cross chooses and Square forgets.

use vitasdk_sys as sys;

pub fn start() {
    unsafe {
        sys::sceCtrlSetSamplingMode(sys::SCE_CTRL_MODE_DIGITAL);
        sys::sceTouchSetSamplingState(sys::SCE_TOUCH_PORT_FRONT, sys::SCE_TOUCH_SAMPLING_STATE_START);
    }
}

/// What is held down right now.
#[derive(Clone, Copy, Default)]
pub struct Held {
    pub talk: bool,
    pub up: bool,
    pub down: bool,
    pub dismiss: bool,
    pub text: bool,
    pub menu: bool,
    pub confirm: bool,
    pub forget: bool,
}

pub fn held() -> Held {
    let mut pad: sys::SceCtrlData = unsafe { core::mem::zeroed() };
    let mut touch: sys::SceTouchData = unsafe { core::mem::zeroed() };
    unsafe {
        sys::sceCtrlPeekBufferPositive(0, &mut pad, 1);
        sys::sceTouchPeek(sys::SCE_TOUCH_PORT_FRONT, &mut touch, 1);
    }
    let down = |button: u32| pad.buttons & button != 0;
    Held {
        talk: down(sys::SCE_CTRL_RTRIGGER) || touch.reportNum > 0,
        up: down(sys::SCE_CTRL_UP),
        down: down(sys::SCE_CTRL_DOWN),
        dismiss: down(sys::SCE_CTRL_CIRCLE),
        text: down(sys::SCE_CTRL_TRIANGLE),
        menu: down(sys::SCE_CTRL_SELECT),
        confirm: down(sys::SCE_CTRL_CROSS),
        forget: down(sys::SCE_CTRL_SQUARE),
    }
}
