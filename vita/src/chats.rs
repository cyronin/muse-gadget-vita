//! The chats the Vita can talk in: Muse's main chat, and side chats the user
//! starts and picks from the menu on Select. A side chat is only an id that
//! the Vita sends with each question; Muse starts the chat the first time it
//! sees one. Muse refuses ids that are not UUIDs, so a new side chat gets a
//! random one and a short name to show. The list and the choice are kept on
//! the memory card.

use crate::files;
use std::sync::Mutex;

const FILE: &str = "state/chats.txt";
/// The most side chats kept. Forgetting one only drops it from this list.
pub const MOST: usize = 8;
const LONGEST_NAME: usize = 24;

/// The side chat questions go to now, for the connection's thread to read.
static CURRENT: Mutex<Option<String>> = Mutex::new(None);

/// The id of the side chat to ask in, or `None` for the main chat.
pub fn current() -> Option<String> {
    CURRENT.lock().unwrap().clone()
}

struct Side {
    id: String,
    name: String,
}

pub struct Chats {
    /// The side chats, oldest first.
    sides: Vec<Side>,
    /// The one in use, as an index into `sides`; `None` is the main chat.
    chosen: Option<usize>,
    /// The number the next new side chat is named with.
    next: u32,
}

/// Whether `id` is a UUID: 36 characters, hex digits and dashes in the usual places.
fn is_uuid(id: &str) -> bool {
    id.len() == 36 && id.bytes().enumerate().all(|(at, b)| if [8, 13, 18, 23].contains(&at) { b == b'-' } else { b.is_ascii_hexdigit() })
}

/// `text` as a name: printable ASCII only, so the font can draw it.
fn clean(text: &str) -> String {
    text.chars().filter(|c| (' '..='~').contains(c)).take(LONGEST_NAME).collect::<String>().trim().into()
}

impl Default for Chats {
    fn default() -> Chats {
        Chats { sides: Vec::new(), chosen: None, next: 1 }
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
    /// for the next new chat, then a line for each side chat: its id, a space
    /// and its name. A line that does not start with a UUID is left out.
    fn parse(text: &str) -> Chats {
        let mut lines = text.lines();
        let chosen = lines.next().unwrap_or("").trim().to_string();
        let mut next: u32 = lines.next().and_then(|line| line.trim().parse().ok()).unwrap_or(1).max(1);
        let mut sides: Vec<Side> = Vec::new();
        for line in lines {
            let (id, name) = line.trim().split_once(' ').unwrap_or((line.trim(), ""));
            if is_uuid(id) && sides.len() < MOST && !sides.iter().any(|side| side.id == id) {
                let name = clean(name);
                let name = if name.is_empty() { format!("Chat {}", &id[..8]) } else { name };
                sides.push(Side { id: id.to_ascii_lowercase(), name });
            }
        }
        // A number already in a name is not given out again.
        for side in &sides {
            if let Some(used) = side.name.strip_prefix("Side chat ").and_then(|digits| digits.parse::<u32>().ok()) {
                next = next.max(used.saturating_add(1));
            }
        }
        let chosen = sides.iter().position(|side| side.id == chosen.to_ascii_lowercase());
        Chats { sides, chosen, next }
    }

    fn render(&self) -> String {
        let mut text = format!("{}\n{}\n", self.chosen.map_or("", |at| self.sides[at].id.as_str()), self.next);
        for side in &self.sides {
            text.push_str(&format!("{} {}\n", side.id, side.name));
        }
        text
    }

    fn publish(&self) {
        *CURRENT.lock().unwrap() = self.chosen.map(|at| self.sides[at].id.clone());
    }

    fn save(&self) {
        if !files::write(FILE, self.render().as_bytes()) {
            crate::say!("chats: could not save {}", FILE);
        }
    }

    /// The rows of the menu: the main chat, the side chats and, while there
    /// is room, a row to start a new one.
    pub fn rows(&self) -> usize {
        1 + self.sides.len() + usize::from(self.sides.len() < MOST)
    }

    /// The row of the chat in use.
    pub fn chosen_row(&self) -> usize {
        self.chosen.map_or(0, |at| at + 1)
    }

    pub fn label(&self, row: usize) -> String {
        match row {
            0 => "Main chat".into(),
            row if row <= self.sides.len() => self.sides[row - 1].name.clone(),
            _ => "+ New side chat".into(),
        }
    }

    /// What to show on the screen for the chat in use; nothing for the main chat.
    pub fn badge(&self) -> String {
        self.chosen.map_or_else(String::new, |at| self.sides[at].name.clone())
    }

    /// Switches to the chat on `row`, starting a new side chat for the last row.
    pub fn choose(&mut self, row: usize) {
        self.choose_with(row, muse_link::new_chat_id);
    }

    fn choose_with(&mut self, row: usize, new_id: impl FnOnce() -> Option<String>) {
        if row == 0 {
            self.chosen = None;
        } else if row <= self.sides.len() {
            self.chosen = Some(row - 1);
        } else if self.sides.len() < MOST {
            let Some(id) = new_id() else {
                crate::say!("chats: no random id for a new side chat");
                return;
            };
            self.sides.push(Side { id, name: format!("Side chat {}", self.next) });
            self.next = self.next.saturating_add(1);
            self.chosen = Some(self.sides.len() - 1);
        }
        self.save();
        self.publish();
    }

    /// Drops the side chat on `row` from the list. Muse keeps the chat.
    pub fn forget(&mut self, row: usize) {
        if row == 0 || row > self.sides.len() {
            return;
        }
        self.sides.remove(row - 1);
        self.chosen = match self.chosen {
            Some(at) if at == row - 1 => None,
            Some(at) if at > row - 1 => Some(at - 1),
            other => other,
        };
        self.save();
        self.publish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "6f1c2d4e-0b7a-4c3e-9f5d-2a8b1e0c7d93";
    const B: &str = "11111111-2222-4333-8444-555555555555";

    fn ids(chats: &Chats) -> Vec<&str> {
        chats.sides.iter().map(|side| side.id.as_str()).collect()
    }

    fn start(chats: &mut Chats, id: &str) {
        let row = chats.rows() - 1;
        chats.choose_with(row, || Some(id.to_string()));
    }

    #[test]
    fn a_new_side_chat_gets_a_name_and_becomes_current() {
        let mut chats = Chats::default();
        assert_eq!((chats.rows(), chats.chosen_row()), (2, 0));
        start(&mut chats, A);
        start(&mut chats, B);
        assert_eq!(ids(&chats), [A, B]);
        assert_eq!((chats.chosen_row(), chats.badge()), (2, "Side chat 2".to_string()));
        assert_eq!(chats.label(3), "+ New side chat");
        chats.choose(0);
        assert_eq!(chats.badge(), "");
    }

    #[test]
    fn a_failed_id_starts_nothing() {
        let mut chats = Chats::default();
        chats.choose_with(1, || None);
        assert_eq!((chats.rows(), chats.chosen_row()), (2, 0));
    }

    #[test]
    fn a_forgotten_number_is_not_given_out_again() {
        let mut chats = Chats::default();
        start(&mut chats, A);
        start(&mut chats, B);
        chats.forget(2);
        start(&mut chats, "99999999-9999-4999-8999-999999999999");
        assert_eq!(chats.sides[1].name, "Side chat 3");
        let again = Chats::parse(&chats.render());
        assert_eq!((again.sides.len(), again.chosen, again.next), (2, Some(1), 4));
        assert_eq!((ids(&again), again.badge()), (ids(&chats), "Side chat 3".to_string()));
    }

    #[test]
    fn forgetting_keeps_the_choice_pointing_at_the_same_chat() {
        let mut chats = Chats::parse(&format!("{B}\n1\n{A} a\n{B} b\n99999999-9999-4999-8999-999999999999 c\n"));
        assert_eq!(chats.chosen, Some(1));
        chats.forget(1);
        assert_eq!((chats.chosen, chats.badge()), (Some(0), "b".to_string()));
        chats.forget(1);
        assert_eq!(chats.chosen, None);
        chats.forget(0);
        chats.forget(9);
        assert_eq!(chats.sides.len(), 1);
    }

    #[test]
    fn only_uuids_are_kept_and_the_list_is_capped() {
        assert!(is_uuid(A) && !is_uuid("vita-1") && !is_uuid(&A.replace('-', "x")));
        // The ids an earlier version made were refused by Muse, so they are dropped.
        let old = Chats::parse("vita-2\n3\nvita-1\nvita-2\n");
        assert_eq!((old.sides.len(), old.chosen, old.next), (0, None, 3));
        let many: String = (0..20).map(|n| format!("{n:08}-2222-4333-8444-555555555555 chat{n}\n")).collect();
        let full = Chats::parse(&format!("\n1\n{many}"));
        assert_eq!((full.sides.len(), full.rows(), full.label(0)), (MOST, MOST + 1, "Main chat".to_string()));
        let odd = Chats::parse(&format!("\nx\n{A}\n{A} twice\n"));
        assert_eq!((odd.sides.len(), odd.badge().as_str(), odd.next), (1, "", 1));
        assert_eq!(odd.label(1), "Chat 6f1c2d4e");
        assert_eq!(Chats::parse("").sides.len(), 0);
    }
}
