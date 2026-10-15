//! The talk button: the right shoulder button, or a finger anywhere on the
//! front touchscreen.

use vitasdk_sys as sys;

pub fn start() {
    unsafe {
        sys::sceCtrlSetSamplingMode(sys::SCE_CTRL_MODE_DIGITAL);
        sys::sceTouchSetSamplingState(sys::SCE_TOUCH_PORT_FRONT, sys::SCE_TOUCH_SAMPLING_STATE_START);
    }
}

/// Whether the user is holding the talk button right now.
pub fn talking() -> bool {
    let mut pad: sys::SceCtrlData = unsafe { core::mem::zeroed() };
    let mut touch: sys::SceTouchData = unsafe { core::mem::zeroed() };
    unsafe {
        sys::sceCtrlPeekBufferPositive(0, &mut pad, 1);
        sys::sceTouchPeek(sys::SCE_TOUCH_PORT_FRONT, &mut touch, 1);
    }
    pad.buttons & sys::SCE_CTRL_RTRIGGER != 0 || touch.reportNum > 0
}
