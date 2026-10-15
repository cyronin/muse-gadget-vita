//! The buttons: the right shoulder button or a finger on the front
//! touchscreen to talk, Up and Down to scroll a reply, Circle to put it away.

use vitasdk_sys as sys;

pub fn start() {
    unsafe {
        sys::sceCtrlSetSamplingMode(sys::SCE_CTRL_MODE_DIGITAL);
        sys::sceTouchSetSamplingState(sys::SCE_TOUCH_PORT_FRONT, sys::SCE_TOUCH_SAMPLING_STATE_START);
    }
}

/// What is held down right now.
pub struct Held {
    pub talk: bool,
    pub up: bool,
    pub down: bool,
    pub dismiss: bool,
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
    }
}
