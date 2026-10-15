//! The chats the Vita can talk in: Muse's main chat, and side chats the user
//! starts and picks from the menu on Triangle. A side chat is only an id
//! that the Vita sends with each question; Muse starts the chat the first
//! time it sees one. The list and the choice are kept on the memory card.

use crate::files;
use std::sync::Mutex;

const FILE: &str = "state/chats.txt";
/// The most side chats kept. Forgetting one only drops it from this list.
pub const MOST: usize = 8;
const LONGEST_ID: usize = 32;
const PREFIX: &str = "vita-";

/// The side chat questions go to now, for the connection's thread to read.
static CURRENT: Mutex<Option<String>> = Mutex::new(None);

/// The id of the side chat to ask in, or `None` for the main chat.
pub fn current() -> Option<String> {
    CURRENT.lock().unwrap().clone()
}

pub struct Chats {
    /// The side chats, oldest first.
    ids: Vec<String>,
    /// The one in use, as an index into `ids`; `None` is the main chat.
    chosen: Option<usize>,
    /// The number the next new side chat is given.
    next: u32,
}

/// `text` as an id: printable ASCII only, so the font can draw it.
fn clean(text: &str) -> String {
    text.trim().chars().filter(|c| (' '..='~').contains(c)).take(LONGEST_ID).collect::<String>().trim().into()
}

fn number(id: &str) -> Option<u32> {
    id.strip_prefix(PREFIX).filter(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())).and_then(|digits| digits.parse().ok())
}

impl Default for Chats {
    fn default() -> Chats {
        Chats { ids: Vec::new(), chosen: None, next: 1 }
    }
}

impl Chats {
    /// Reads the list from the card, and makes its choice the current one.
    pub fn load() -> Chats {
        let chats = files::read(FILE).and_then(|file| String::from_utf8(file).ok()).map_or_else(Chats::default, |text| Chats::parse(&text));
        chats.publish();
        chats
    }

    /// The file is the chosen id (a blank line for the main chat), the number
    /// for the next new chat, then one id to a line.
    fn parse(text: &str) -> Chats {
        let mut lines = text.lines();
        let chosen = clean(lines.next().unwrap_or(""));
        let mut next: u32 = lines.next().and_then(|line| line.trim().parse().ok()).unwrap_or(1);
        let mut ids: Vec<String> = Vec::new();
        for id in lines.map(clean).chain(Some(chosen.clone())) {
            if !id.is_empty() && !ids.contains(&id) && ids.len() < MOST {
                ids.push(id);
            }
        }
        // A number already in use is never given out again.
        next = ids.iter().filter_map(|id| number(id)).fold(next.max(1), |next, used| next.max(used.saturating_add(1)));
        let chosen = ids.iter().position(|id| *id == chosen);
        Chats { ids, chosen, next }
    }

    fn render(&self) -> String {
        let mut text = format!("{}\n{}\n", self.chosen.map_or("", |at| self.ids[at].as_str()), self.next);
        for id in &self.ids {
            text.push_str(id);
            text.push('\n');
        }
        text
    }

    fn publish(&self) {
        *CURRENT.lock().unwrap() = self.chosen.map(|at| self.ids[at].clone());
    }

    fn save(&self) {
        if !files::write(FILE, self.render().as_bytes()) {
            crate::say!("chats: could not save {}", FILE);
        }
    }

    /// The rows of the menu: the main chat, the side chats and, while there
    /// is room, a row to start a new one.
    pub fn rows(&self) -> usize {
        1 + self.ids.len() + usize::from(self.ids.len() < MOST)
    }

    /// The row of the chat in use.
    pub fn chosen_row(&self) -> usize {
        self.chosen.map_or(0, |at| at + 1)
    }

    pub fn label(&self, row: usize) -> String {
        match row {
            0 => "Main chat".into(),
            row if row <= self.ids.len() => pretty(&self.ids[row - 1]),
            _ => "+ New side chat".into(),
        }
    }

    /// What to show on the screen for the chat in use; nothing for the main chat.
    pub fn badge(&self) -> String {
        self.chosen.map_or_else(String::new, |at| pretty(&self.ids[at]))
    }

    /// Switches to the chat on `row`, starting a new side chat for the last row.
    pub fn choose(&mut self, row: usize) {
        if row == 0 {
            self.chosen = None;
        } else if row <= self.ids.len() {
            self.chosen = Some(row - 1);
        } else if self.ids.len() < MOST {
            self.ids.push(format!("{PREFIX}{}", self.next));
            self.next = self.next.saturating_add(1);
            self.chosen = Some(self.ids.len() - 1);
        }
        self.save();
        self.publish();
    }

    /// Drops the side chat on `row` from the list. Muse keeps the chat.
    pub fn forget(&mut self, row: usize) {
        if row == 0 || row > self.ids.len() {
            return;
        }
        self.ids.remove(row - 1);
        self.chosen = match self.chosen {
            Some(at) if at == row - 1 => None,
            Some(at) if at > row - 1 => Some(at - 1),
            other => other,
        };
        self.save();
        self.publish();
    }
}

/// An id as the menu names it: "vita-3" is "Side chat 3".
fn pretty(id: &str) -> String {
    match number(id) {
        Some(n) => format!("Side chat {n}"),
        None => id.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_side_chat_gets_the_next_number_and_becomes_current() {
        let mut chats = Chats::default();
        assert_eq!((chats.rows(), chats.chosen_row()), (2, 0));
        chats.choose(1);
        chats.choose(2);
        assert_eq!(chats.ids, ["vita-1", "vita-2"]);
        assert_eq!((chats.chosen_row(), chats.badge()), (2, "Side chat 2".to_string()));
        assert_eq!(chats.label(3), "+ New side chat");
        chats.choose(0);
        assert_eq!(chats.badge(), "");
    }

    #[test]
    fn a_forgotten_number_is_not_given_out_again() {
        let mut chats = Chats::default();
        chats.choose(1);
        chats.choose(2);
        chats.forget(2);
        chats.choose(chats.rows() - 1);
        assert_eq!(chats.ids, ["vita-1", "vita-3"]);
        let again = Chats::parse(&chats.render());
        assert_eq!((again.ids, again.chosen, again.next), (chats.ids.clone(), Some(1), 4));
    }

    #[test]
    fn forgetting_keeps_the_choice_pointing_at_the_same_chat() {
        let mut chats = Chats::parse("c\n1\na\nb\nc\n");
        assert_eq!(chats.chosen, Some(2));
        chats.forget(1);
        assert_eq!((chats.ids.clone(), chats.chosen), (vec!["b".to_string(), "c".to_string()], Some(1)));
        chats.forget(2);
        assert_eq!(chats.chosen, None);
        chats.forget(0);
        chats.forget(9);
        assert_eq!(chats.ids, ["b"]);
    }

    #[test]
    fn the_list_is_capped_and_a_stray_file_is_made_safe() {
        let many: String = (0..20).map(|n| format!("chat{n}\n")).collect();
        assert_eq!(Chats::parse(&format!("\n1\n{many}")).ids.len(), MOST);
        let full = Chats::parse(&format!("\n1\n{many}"));
        assert_eq!((full.rows(), full.label(0)), (MOST + 1, "Main chat".to_string()));
        let odd = Chats::parse("caf\u{e9}\u{7}x\nnot a number\n\nvita-7\nvita-7\n");
        assert_eq!(odd.ids, ["vita-7", "cafx"]);
        assert_eq!((odd.chosen, odd.next), (Some(1), 8));
        assert_eq!(Chats::parse("").ids.len(), 0);
    }
}
