//! The last few caption lines, kept so an answer can be given in context.

use std::collections::VecDeque;

use crate::prompt::Turn;

/// Lines kept. More than the prompt uses, so the line of the question itself can be dropped
/// from the context without losing a useful one.
const CAPACITY: usize = 12;

#[derive(Debug, Default)]
pub struct History {
    turns: VecDeque<Turn>,
}

impl History {
    /// Adds a line. Blank text is ignored, and the oldest line is dropped when full.
    pub fn push(&mut self, from_me: bool, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        if self.turns.len() == CAPACITY {
            self.turns.pop_front();
        }
        self.turns.push_back(Turn::new(from_me, text));
    }

    /// The lines kept, oldest first.
    pub fn snapshot(&self) -> Vec<Turn> {
        self.turns.iter().cloned().collect()
    }

    pub fn clear(&mut self) {
        self.turns.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_come_back_oldest_first() {
        let mut history = History::default();
        history.push(false, "một");
        history.push(true, "hai");
        assert_eq!(
            history.snapshot(),
            [Turn::new(false, "một"), Turn::new(true, "hai")]
        );
    }

    #[test]
    fn blank_lines_are_ignored_and_text_is_trimmed() {
        let mut history = History::default();
        history.push(false, "   ");
        history.push(false, "  xin chào  ");
        assert_eq!(history.snapshot(), [Turn::new(false, "xin chào")]);
    }

    #[test]
    fn the_oldest_line_is_dropped_when_full() {
        let mut history = History::default();
        for i in 0..CAPACITY + 3 {
            history.push(false, &format!("dòng {i}"));
        }
        let lines = history.snapshot();
        assert_eq!(lines.len(), CAPACITY);
        assert_eq!(lines[0].text, "dòng 3");
        assert_eq!(lines[CAPACITY - 1].text, format!("dòng {}", CAPACITY + 2));
    }

    #[test]
    fn clearing_forgets_everything() {
        let mut history = History::default();
        history.push(false, "a");
        history.clear();
        assert!(history.snapshot().is_empty());
    }
}
