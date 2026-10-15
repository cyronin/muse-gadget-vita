//! What Muse heard and what it answered, written out beside the avatar, for
//! when the reply cannot be spoken or the user would rather read it.

use crate::font::{self, CELL};
use crate::screen::{self, colour, Screen, BLACK};
use std::sync::Mutex;

/// Where the text goes: right of the half-size avatar, to the screen's edge.
pub const LEFT: usize = 336;
const TOP: usize = 32;
const COLUMNS: usize = (screen::WIDTH - 16 - LEFT) / CELL;
/// The last column is kept for the marks that say there is more.
const TEXT_COLUMNS: usize = COLUMNS - 1;
const LINE: usize = CELL + 4;
const ROWS: usize = (screen::HEIGHT - 2 * TOP) / LINE;
/// How often a held Up or Down moves the text a line.
const SCROLL_EVERY_MS: u32 = 80;

const HEARD_COLOUR: u32 = colour(150, 150, 160);
const REPLY_COLOUR: u32 = colour(235, 235, 240);
const MORE_COLOUR: u32 = colour(255, 180, 40);

#[derive(Default)]
struct Shared {
    heard: String,
    reply: String,
    /// Moves on with every change, so the screen loop wraps the text only then.
    version: u32,
}

/// Written by the connection's thread, read by the screen loop.
static SHARED: Mutex<Shared> = Mutex::new(Shared { heard: String::new(), reply: String::new(), version: 0 });

fn change(apply: impl FnOnce(&mut Shared)) {
    let mut shared = SHARED.lock().unwrap();
    apply(&mut shared);
    shared.version = shared.version.wrapping_add(1);
}

/// What Muse heard in the question just asked. Starts a new exchange.
pub fn heard(text: &str) {
    change(|shared| {
        shared.heard = text.trim().into();
        shared.reply.clear();
    });
}

/// The reply so far.
pub fn reply(text: &str) {
    change(|shared| shared.reply = text.trim().into());
}

/// Something Muse said without being asked.
pub fn said(text: &str) {
    change(|shared| {
        shared.heard.clear();
        shared.reply = text.trim().into();
    });
}

pub fn clear() {
    change(|shared| *shared = Shared { version: shared.version, ..Shared::default() });
}

/// A character the font has, for one of `text`. The font is ASCII where it
/// matters, so typographic marks become their plain forms.
fn plain(character: char) -> &'static str {
    match character {
        '\u{2018}' | '\u{2019}' | '\u{2032}' => "'",
        '\u{201c}' | '\u{201d}' | '\u{2033}' => "\"",
        '\u{2013}' | '\u{2014}' | '\u{2212}' => "-",
        '\u{2026}' => "...",
        '\u{2022}' | '\u{00b7}' => "*",
        '\u{00a0}' | '\t' => " ",
        'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => "a",
        'è' | 'é' | 'ê' | 'ë' => "e",
        'ì' | 'í' | 'î' | 'ï' => "i",
        'ò' | 'ó' | 'ô' | 'ö' | 'õ' => "o",
        'ù' | 'ú' | 'û' | 'ü' => "u",
        'ç' => "c",
        'ñ' => "n",
        'À' | 'Á' | 'Â' | 'Ä' => "A",
        'É' | 'È' => "E",
        'Ö' => "O",
        'Ü' => "U",
        _ => "",
    }
}

/// `text` as the font can draw it, with emphasis marks left out.
fn printable(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.replace("**", "").replace("__", "").chars() {
        match character {
            ' '..='~' | '\n' => out.push(character),
            other => out.push_str(plain(other)),
        }
    }
    out
}

/// `text` broken into lines of at most `TEXT_COLUMNS`, at spaces where it can be.
fn wrap(text: &str, into: &mut Vec<String>) {
    for paragraph in printable(text).split('\n') {
        let mut line = String::new();
        for word in paragraph.split(' ').filter(|word| !word.is_empty()) {
            let mut word = word;
            if !line.is_empty() && line.len() + 1 + word.len() > TEXT_COLUMNS {
                into.push(std::mem::take(&mut line));
            }
            // A word longer than a line is cut where the line ends.
            while word.len() > TEXT_COLUMNS {
                if !line.is_empty() {
                    into.push(std::mem::take(&mut line));
                }
                into.push(word[..TEXT_COLUMNS].into());
                word = &word[TEXT_COLUMNS..];
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        into.push(line);
    }
}

/// The screen loop's copy of the text, wrapped, and how far it is scrolled.
pub struct Panel {
    version: u32,
    lines: Vec<(String, u32)>,
    scroll: usize,
    scrolled_at_ms: u32,
}

impl Panel {
    pub fn new() -> Panel {
        Panel { version: 0, lines: Vec::new(), scroll: 0, scrolled_at_ms: 0 }
    }

    /// Takes up any change to the text. True while there is text to show.
    pub fn update(&mut self) -> bool {
        let shared = SHARED.lock().unwrap();
        if shared.version != self.version {
            self.version = shared.version;
            self.lines.clear();
            let mut lines = Vec::new();
            if !shared.heard.is_empty() {
                wrap(&format!("You: {}", shared.heard), &mut lines);
                self.lines.extend(lines.drain(..).map(|line| (line, HEARD_COLOUR)));
                if !shared.reply.is_empty() {
                    self.lines.push((String::new(), REPLY_COLOUR));
                }
            }
            if !shared.reply.is_empty() {
                wrap(&shared.reply, &mut lines);
                self.lines.extend(lines.drain(..).map(|line| (line, REPLY_COLOUR)));
            }
            self.scroll = self.scroll.min(self.lines.len().saturating_sub(ROWS));
        }
        !self.lines.is_empty()
    }

    /// Moves the text a line while Up or Down is held.
    pub fn scroll(&mut self, up: bool, down: bool, now_ms: u32) {
        if up == down || now_ms.wrapping_sub(self.scrolled_at_ms) < SCROLL_EVERY_MS {
            return;
        }
        self.scrolled_at_ms = now_ms;
        let last = self.lines.len().saturating_sub(ROWS);
        self.scroll = if up { self.scroll.saturating_sub(1) } else { (self.scroll + 1).min(last) };
    }

    pub fn draw(&self, screen: &mut Screen) {
        screen.fill(LEFT, TOP, COLUMNS * CELL, ROWS * LINE, BLACK);
        for (row, (line, colour)) in self.lines.iter().skip(self.scroll).take(ROWS).enumerate() {
            font::draw(screen, LEFT, TOP + row * LINE, line.as_bytes(), *colour);
        }
        // Marks at the edge for text above or below what is shown.
        let edge = LEFT + (COLUMNS - 1) * CELL;
        if self.scroll > 0 {
            font::draw(screen, edge, TOP, b"^", MORE_COLOUR);
        }
        if self.scroll + ROWS < self.lines.len() {
            font::draw(screen, edge, TOP + (ROWS - 1) * LINE, b"v", MORE_COLOUR);
        }
    }
}
