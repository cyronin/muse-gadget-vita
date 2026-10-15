//! A small light and a word in the corner of the screen, for whether Muse
//! can be reached.

use crate::font::{self, CELL, SCALE};
use crate::screen::{colour, Screen, BLACK};

const LEFT: usize = 20;
const TOP: usize = 20;
/// Room for the light and the longest word, in the font's own pixels.
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
    for line in 0..CELL {
        let row = &mut screen.row(TOP + line)[LEFT..LEFT + WIDEST * SCALE];
        row.fill(BLACK);
        // The light is six font pixels round, with its corners cut.
        let design_line = line / SCALE;
        if lit && (1..7).contains(&design_line) {
            let inset = usize::from(design_line == 1 || design_line == 6);
            row[(1 + inset) * SCALE..(7 - inset) * SCALE].fill(link.light());
        }
    }
    font::draw(screen, LEFT + 12 * SCALE, TOP, link.word().as_bytes(), WORD_COLOUR);
}
