//! The chat menu, opened with Triangle: the main chat and the side chats,
//! with Up and Down to move, Cross to choose, Square to forget a side chat
//! and Circle or Triangle to close it.

use crate::chats::Chats;
use crate::font::{self, CELL};
use crate::input::Held;
use crate::screen::{self, colour, Screen};

const WIDTH: usize = 640;
const LEFT: usize = (screen::WIDTH - WIDTH) / 2;
const PAD: usize = 16;
const LINE: usize = CELL + 4;
const COLUMNS: usize = (WIDTH - 2 * PAD) / CELL;

const BORDER: u32 = colour(90, 90, 120);
const BACKGROUND: u32 = colour(18, 18, 32);
const TITLE_COLOUR: u32 = colour(255, 180, 40);
const ROW_COLOUR: u32 = colour(150, 150, 160);
const PICKED_COLOUR: u32 = colour(235, 235, 240);
const HINT_COLOUR: u32 = colour(110, 110, 125);

/// What a press did, for the screen loop to follow.
#[derive(Clone, Copy, PartialEq)]
pub enum Change {
    Nothing,
    Opened,
    Closed,
    /// A chat was chosen, which may be the one already in use.
    Chose,
}

#[derive(Default)]
pub struct Picker {
    open: bool,
    row: usize,
}

impl Picker {
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Follows the buttons. `before` is what was held a frame ago, so that a
    /// press counts once. The menu only opens when `may_open`.
    pub fn update(&mut self, chats: &mut Chats, held: &Held, before: &Held, may_open: bool) -> Change {
        if !self.open {
            if may_open && held.menu && !before.menu {
                self.open = true;
                self.row = chats.chosen_row();
                return Change::Opened;
            }
            return Change::Nothing;
        }
        let last = chats.rows() - 1;
        if held.up && !before.up {
            self.row = self.row.saturating_sub(1);
        }
        if held.down && !before.down {
            self.row = (self.row + 1).min(last);
        }
        if held.forget && !before.forget {
            chats.forget(self.row);
            self.row = self.row.min(chats.rows() - 1);
        }
        if held.confirm && !before.confirm {
            chats.choose(self.row);
            self.open = false;
            return Change::Chose;
        }
        if (held.dismiss && !before.dismiss) || (held.menu && !before.menu) {
            self.open = false;
            return Change::Closed;
        }
        Change::Nothing
    }

    /// Draws the menu over whatever is on the screen.
    pub fn draw(&self, screen: &mut Screen, chats: &Chats) {
        let rows = chats.rows();
        // The title and a gap, the rows, a gap and two lines of hints.
        let height = 2 * PAD + (rows + 5) * LINE;
        let top = (screen::HEIGHT - height) / 2;
        screen.fill(LEFT - 2, top - 2, WIDTH + 4, height + 4, BORDER);
        screen.fill(LEFT, top, WIDTH, height, BACKGROUND);
        let (text_left, mut y) = (LEFT + PAD, top + PAD);
        font::draw(screen, text_left, y, b"Choose a chat", TITLE_COLOUR);
        y += 2 * LINE;
        for row in 0..rows {
            let picked = row == self.row;
            let in_use = row == chats.chosen_row();
            let line = format!("{} {}{}", if picked { '>' } else { ' ' }, chats.label(row), if in_use { "  (in use)" } else { "" });
            let line = &line.as_bytes()[..line.len().min(COLUMNS)];
            font::draw(screen, text_left, y, line, if picked { PICKED_COLOUR } else { ROW_COLOUR });
            y += LINE;
        }
        y += LINE;
        font::draw(screen, text_left, y, b"Up/Down: move   Cross: choose", HINT_COLOUR);
        font::draw(screen, text_left, y + LINE, b"Square: forget   Circle: back", HINT_COLOUR);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(picker: &mut Picker, chats: &mut Chats, button: impl Fn(&mut Held), may_open: bool) -> Change {
        let mut held = Held::default();
        button(&mut held);
        let change = picker.update(chats, &held, &Held::default(), may_open);
        // Released again, so that the next press is a new one.
        picker.update(chats, &Held::default(), &held, may_open);
        change
    }

    #[test]
    fn the_menu_opens_only_between_questions_and_a_held_button_counts_once() {
        let (mut picker, mut chats) = (Picker::default(), Chats::default());
        assert!(press(&mut picker, &mut chats, |held| held.menu = true, false) == Change::Nothing && !picker.is_open());
        assert!(press(&mut picker, &mut chats, |held| held.menu = true, true) == Change::Opened && picker.is_open());
        let held = Held { menu: true, ..Held::default() };
        assert!(picker.update(&mut chats, &held, &held, true) == Change::Nothing && picker.is_open());
    }

    #[test]
    fn down_and_cross_start_a_side_chat_and_circle_leaves_the_choice_alone() {
        let (mut picker, mut chats) = (Picker::default(), Chats::default());
        press(&mut picker, &mut chats, |held| held.menu = true, true);
        press(&mut picker, &mut chats, |held| held.down = true, true);
        press(&mut picker, &mut chats, |held| held.down = true, true);
        assert!(press(&mut picker, &mut chats, |held| held.confirm = true, true) == Change::Chose);
        assert!(!picker.is_open());
        assert_eq!(chats.badge(), "Side chat 1");

        press(&mut picker, &mut chats, |held| held.menu = true, true);
        assert_eq!(picker.row, 1);
        press(&mut picker, &mut chats, |held| held.up = true, true);
        assert!(press(&mut picker, &mut chats, |held| held.dismiss = true, true) == Change::Closed);
        assert_eq!(chats.badge(), "Side chat 1");

        press(&mut picker, &mut chats, |held| held.menu = true, true);
        press(&mut picker, &mut chats, |held| held.up = true, true);
        press(&mut picker, &mut chats, |held| held.up = true, true);
        assert_eq!(picker.row, 0);
        press(&mut picker, &mut chats, |held| held.confirm = true, true);
        assert_eq!(chats.badge(), "");
    }

    #[test]
    fn square_forgets_the_side_chat_under_the_cursor_and_never_the_main_chat() {
        let (mut picker, mut chats) = (Picker::default(), Chats::default());
        chats.choose(1);
        press(&mut picker, &mut chats, |held| held.menu = true, true);
        assert_eq!(picker.row, 1);
        press(&mut picker, &mut chats, |held| held.forget = true, true);
        assert_eq!((chats.rows(), chats.badge(), picker.row), (2, String::new(), 1));
        press(&mut picker, &mut chats, |held| held.up = true, true);
        press(&mut picker, &mut chats, |held| held.forget = true, true);
        assert_eq!(chats.label(0), "Main chat");
    }
}
