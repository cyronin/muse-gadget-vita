//! rust-psp's debug font, drawn at twice its size for the Vita's screen.

use crate::screen::Screen;

/// 256 characters, eight rows of eight pixels each, the left pixel in the
/// high bit.
const FONT: &[u8; 2048] = include_bytes!("msxfont.bin");

/// Screen pixels per font pixel. The Vita's screen has twice the PSP's
/// pixels each way, so this keeps text the size it was there.
pub const SCALE: usize = 2;
/// The size of one character on the screen.
pub const CELL: usize = 8 * SCALE;

/// Draws `text`, one byte per character, with its top left at `left`,
/// `top`. Only the lit pixels are drawn; the caller clears behind it.
pub fn draw(screen: &mut Screen, left: usize, top: usize, text: &[u8], colour: u32) {
    for line in 0..8 {
        for copy in 0..SCALE {
            let row = screen.row(top + line * SCALE + copy);
            for (place, &character) in text.iter().enumerate() {
                let bits = FONT[character as usize * 8 + line];
                if bits == 0 {
                    continue;
                }
                let at = left + place * CELL;
                for pixel in 0..8 {
                    if bits & (0x80 >> pixel) != 0 {
                        row[at + pixel * SCALE..][..SCALE].fill(colour);
                    }
                }
            }
        }
    }
}
