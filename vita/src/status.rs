//! A small light and a word in the corner of the screen, for whether Muse
//! can be reached.

use crate::screen::{colour, Screen, BLACK};

/// rust-psp's debug font: 256 characters, eight rows of eight pixels each,
/// the left pixel in the high bit.
const FONT: &[u8; 2048] = include_bytes!("msxfont.bin");

/// Screen pixels per font pixel. The Vita's screen has twice the PSP's
/// pixels each way, so this keeps the corner the size it was there.
const SCALE: usize = 2;
const LEFT: usize = 20;
const TOP: usize = 20;
const WIDEST: usize = 14 * 8;
const WORD_COLOUR: u32 = colour(150, 150, 160);

#[derive(Clone, Copy, PartialEq)]
pub enum Link {
    Connecting,
    Online,
    Offline,
}

impl Link {
    fn word(self) -> &'static str {
        match self {
            Link::Connecting => "connecting",
            Link::Online => "online",
            Link::Offline => "offline",
        }
    }

    fn light(self) -> u32 {
        match self {
            Link::Connecting => colour(255, 180, 40),
            Link::Online => colour(70, 220, 110),
            Link::Offline => colour(255, 80, 80),
        }
    }
}

/// Draws the light and the word. `now_ms` makes the light blink while
/// connecting.
pub fn draw(screen: &mut Screen, link: Link, now_ms: u32) {
    let lit = link != Link::Connecting || now_ms / 500 % 2 == 0;
    for line in 0..8 {
        // One line of the PSP's 8-pixel design, as the PSP drew it.
        let mut design = [BLACK; WIDEST];
        if lit && (1..7).contains(&line) {
            let inset = usize::from(line == 1 || line == 6);
            design[1 + inset..7 - inset].fill(link.light());
        }
        for (place, character) in link.word().bytes().enumerate() {
            let bits = FONT[character as usize * 8 + line];
            for pixel in 0..8 {
                if bits & (0x80 >> pixel) != 0 {
                    design[12 + place * 8 + pixel] = WORD_COLOUR;
                }
            }
        }
        for copy in 0..SCALE {
            let row = &mut screen.row(TOP + line * SCALE + copy)[LEFT..LEFT + WIDEST * SCALE];
            for (pixels, &colour) in row.chunks_mut(SCALE).zip(&design) {
                pixels.fill(colour);
            }
        }
    }
}
