//! The Vita's display as a plain 960 by 544 grid of 32-bit colours, drawn
//! into one of two buffers while the display shows the other.

use std::ffi::c_void;
use vitasdk_sys as sys;

pub const WIDTH: usize = 960;
pub const HEIGHT: usize = 544;
/// Pixels from the start of one row to the start of the next. The display
/// wants a multiple of 64, which the width already is.
const STRIDE: usize = WIDTH;
/// Video memory comes in blocks of 256 KiB.
const BUFFER_BYTES: usize = (STRIDE * HEIGHT * 4).next_multiple_of(256 * 1024);

/// A colour as the screen holds it: red in the low byte, then green, blue
/// and an opaque alpha.
pub const fn colour(red: u8, green: u8, blue: u8) -> u32 {
    red as u32 | (green as u32) << 8 | (blue as u32) << 16 | 0xff00_0000
}

pub const BLACK: u32 = colour(0, 0, 0);

pub struct Screen {
    buffers: [*mut u32; 2],
    /// The buffer being drawn, the one the display is not showing.
    back: usize,
    /// Buffers still holding a layout the screen has moved on from.
    stale: [bool; 2],
}

fn buffer(name: &[u8]) -> *mut u32 {
    unsafe {
        let block = sys::sceKernelAllocMemBlock(
            name.as_ptr() as *const _,
            sys::SCE_KERNEL_MEMBLOCK_TYPE_USER_CDRAM_RW,
            BUFFER_BYTES as u32,
            core::ptr::null_mut(),
        );
        assert!(block >= 0, "no video memory for the screen: {:#x}", block);
        let mut base: *mut c_void = core::ptr::null_mut();
        sys::sceKernelGetMemBlockBase(block, &mut base);
        let pixels = base as *mut u32;
        core::slice::from_raw_parts_mut(pixels, STRIDE * HEIGHT).fill(BLACK);
        pixels
    }
}

impl Screen {
    pub fn open() -> Screen {
        let mut screen = Screen { buffers: [buffer(b"screen0\0"), buffer(b"screen1\0")], back: 1, stale: [false; 2] };
        screen.show(0);
        screen
    }

    fn show(&mut self, which: usize) {
        let frame = sys::SceDisplayFrameBuf {
            size: core::mem::size_of::<sys::SceDisplayFrameBuf>() as u32,
            base: self.buffers[which] as *mut c_void,
            pitch: STRIDE as u32,
            pixelformat: sys::SCE_DISPLAY_PIXELFORMAT_A8B8G8R8,
            width: WIDTH as u32,
            height: HEIGHT as u32,
        };
        let set = unsafe { sys::sceDisplaySetFrameBuf(&frame, sys::SCE_DISPLAY_SETBUF_NEXTFRAME) };
        if set < 0 {
            crate::say!("screen: display refused the frame: {:#x}", set);
        }
    }

    /// Marks both buffers for clearing, when what is drawn where changes.
    pub fn wipe(&mut self) {
        self.stale = [true; 2];
    }

    /// Call before drawing a frame. Clears the buffer about to be drawn if
    /// it still holds the last layout.
    pub fn begin(&mut self) {
        if core::mem::take(&mut self.stale[self.back]) {
            unsafe { core::slice::from_raw_parts_mut(self.buffers[self.back], STRIDE * HEIGHT) }.fill(BLACK);
        }
    }

    /// Fills `width` pixels of `height` rows from `left`, `top`.
    pub fn fill(&mut self, left: usize, top: usize, width: usize, height: usize, colour: u32) {
        for y in top..top + height {
            self.row(y)[left..left + width].fill(colour);
        }
    }

    pub fn row(&mut self, y: usize) -> &mut [u32] {
        assert!(y < HEIGHT);
        unsafe { core::slice::from_raw_parts_mut(self.buffers[self.back].add(y * STRIDE), WIDTH) }
    }

    /// Shows what was drawn from the next frame on, and waits for it. Every
    /// frame of one layout draws the same places, so each buffer stays whole.
    pub fn next_frame(&mut self) {
        self.show(self.back);
        unsafe { sys::sceDisplayWaitVblankStart() };
        self.back ^= 1;
    }
}
