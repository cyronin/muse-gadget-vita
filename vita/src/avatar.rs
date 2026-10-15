//! The Muse Gadget SDK's avatar, played from the clips tools/avatar.sh
//! renders with the SDK's own code. The file layout is described in
//! tools/render_avatar.c.

use crate::screen::{self, Screen};

/// What the avatar is doing. The order is the order of clips in the file.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mood {
    Boot,
    Idle,
    Listening,
    Thinking,
    Speaking,
    Error,
    /// Not shown by this app. It is here because the file's clips are in this order.
    #[allow(dead_code)]
    Off,
    Happy,
}

/// How many clips the file must hold.
const MOODS: usize = Mood::Happy as usize + 1;

struct Clip {
    /// The clip's first frame in `Avatar::frames`.
    first: usize,
    frames: usize,
    loops: bool,
}

struct Frame {
    /// Each colour as the screen holds it, and its edge colour.
    plain: Vec<u32>,
    edge: Vec<u32>,
    /// Where the frame's pixels start in the file.
    pixels: usize,
}

pub struct Avatar {
    file: Vec<u8>,
    side: usize,
    frames_per_second: usize,
    clips: Vec<Clip>,
    frames: Vec<Frame>,
}

/// Screen pixels per avatar pixel at its largest. The last row and column
/// of each block are drawn in the edge colour, which is the SDK's grid
/// texture. Twice the PSP's, so the avatar fills the same share of the
/// larger screen.
pub const BLOCK: usize = 8;

/// Where the avatar goes on the screen, and how large.
#[derive(Clone, Copy, PartialEq)]
pub struct Place {
    pub left: usize,
    pub top: usize,
    /// Screen pixels per avatar pixel.
    pub block: usize,
}

/// A colour from the file, which holds the PSP's 16 bits with red low.
fn from_psp(colour: u16) -> u32 {
    let widen = |value: u16, bits: u32| {
        let value = value as u32;
        (value << (8 - bits) | value >> (2 * bits - 8)) as u8
    };
    screen::colour(widen(colour & 0x1f, 5), widen(colour >> 5 & 0x3f, 6), widen(colour >> 11, 5))
}

impl Avatar {
    /// Reads the clips, or `None` when the file is not one this app wrote.
    pub fn parse(file: Vec<u8>) -> Option<Avatar> {
        let word = |at: usize| Some(u16::from_le_bytes([*file.get(at)?, *file.get(at + 1)?]) as usize);
        if file.get(..8)? != b"MUSEAV1\0" {
            return None;
        }
        let (side, height, colours, frames_per_second, count) = (word(8)?, word(10)?, word(12)?, word(14)?, word(16)?);
        if side != height || side * BLOCK > screen::HEIGHT || frames_per_second == 0 || count != MOODS {
            return None;
        }
        let frame_bytes = 4 * colours + side * side;
        let mut start = 18 + 4 * count;
        let mut clips = Vec::with_capacity(count);
        let mut frames = Vec::new();
        for clip in 0..count {
            let length = word(18 + 4 * clip)?;
            if length == 0 {
                return None;
            }
            clips.push(Clip { first: frames.len(), frames: length, loops: word(20 + 4 * clip)? != 0 });
            for _ in 0..length {
                let table = |which: usize| -> Option<Vec<u32>> {
                    (0..colours).map(|index| word(start + 2 * (which * colours + index)).map(|c| from_psp(c as u16))).collect()
                };
                let pixels = start + 4 * colours;
                if file.get(pixels..pixels + side * side)?.iter().any(|&colour| colour as usize >= colours) {
                    return None;
                }
                frames.push(Frame { plain: table(0)?, edge: table(1)?, pixels });
                start += frame_bytes;
            }
        }
        Some(Avatar { file, side, frames_per_second, clips, frames })
    }

    /// Avatar pixels across, and down.
    pub fn side(&self) -> usize {
        self.side
    }

    /// Centred on the screen at the largest size.
    pub fn centred(&self) -> Place {
        let size = self.side * BLOCK;
        Place { left: (screen::WIDTH - size) / 2, top: (screen::HEIGHT - size) / 2, block: BLOCK }
    }

    /// Draws `mood` as it looks `since_ms` after it began, at `place`.
    pub fn draw(&self, screen: &mut Screen, mood: Mood, since_ms: u32, place: Place) {
        let clip = &self.clips[mood as usize];
        let played = since_ms as usize * self.frames_per_second / 1000;
        let frame = &self.frames[clip.first + if clip.loops { played % clip.frames } else { played.min(clip.frames - 1) }];
        let pixels = &self.file[frame.pixels..][..self.side * self.side];
        let Place { left, top, block: size } = place;
        for (y, cells) in pixels.chunks(self.side).enumerate() {
            for line in 0..size {
                let row = &mut screen.row(top + y * size + line)[left..left + self.side * size];
                let edge_line = line == size - 1;
                for (block, &cell) in row.chunks_mut(size).zip(cells) {
                    let (plain, edge) = (frame.plain[cell as usize], frame.edge[cell as usize]);
                    block[..size - 1].fill(if edge_line { edge } else { plain });
                    block[size - 1] = edge;
                }
            }
        }
    }
}
