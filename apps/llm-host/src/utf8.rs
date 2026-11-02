//! Turns the byte pieces a model writes into text. A token can end in the middle of a character
//! (Vietnamese letters with stacked marks are several bytes, and emoji span tokens), so bytes are
//! held back until they form whole characters.

#[derive(Default)]
pub struct Utf8Stream {
    pending: Vec<u8>,
}

impl Utf8Stream {
    /// Adds bytes and returns the text that is complete so far. Bytes that start a character that
    /// has not arrived yet stay until the next call. Bytes that can never be valid become U+FFFD.
    pub fn push(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        let mut out = String::new();
        loop {
            match std::str::from_utf8(&self.pending) {
                Ok(text) => {
                    out.push_str(text);
                    self.pending.clear();
                    return out;
                }
                Err(e) => {
                    let valid = e.valid_up_to();
                    // Not an error here: `valid_up_to` bytes are valid UTF-8 by definition.
                    out.push_str(&String::from_utf8_lossy(&self.pending[..valid]));
                    match e.error_len() {
                        // An invalid sequence in the middle: replace it and go on.
                        Some(len) => {
                            out.push('\u{fffd}');
                            self.pending.drain(..valid + len);
                        }
                        // The end of the input is the start of a character: wait for the rest.
                        None => {
                            self.pending.drain(..valid);
                            return out;
                        }
                    }
                }
            }
        }
    }

    /// What is left at the end of the answer: a character that never completed.
    pub fn finish(&mut self) -> String {
        if self.pending.is_empty() {
            return String::new();
        }
        self.pending.clear();
        "\u{fffd}".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_characters_pass_straight_through() {
        let mut s = Utf8Stream::default();
        assert_eq!(s.push("Xin chào".as_bytes()), "Xin chào");
        assert_eq!(s.finish(), "");
    }

    #[test]
    fn a_character_split_between_two_pieces_is_held_until_complete() {
        let bytes = "ệ".as_bytes(); // three bytes
        let mut s = Utf8Stream::default();
        assert_eq!(s.push(&bytes[..1]), "");
        assert_eq!(s.push(&bytes[1..2]), "");
        assert_eq!(s.push(&bytes[2..]), "ệ");
    }

    #[test]
    fn text_before_a_partial_character_is_not_held_back() {
        let bytes = "aệ".as_bytes();
        let mut s = Utf8Stream::default();
        assert_eq!(s.push(&bytes[..2]), "a");
        assert_eq!(s.push(&bytes[2..]), "ệ");
    }

    #[test]
    fn an_invalid_byte_becomes_a_replacement_and_the_rest_goes_on() {
        let mut s = Utf8Stream::default();
        assert_eq!(s.push(&[b'a', 0xff, b'b']), "a\u{fffd}b");
    }

    #[test]
    fn a_character_that_never_completes_is_reported_at_the_end() {
        let mut s = Utf8Stream::default();
        assert_eq!(s.push(&"ệ".as_bytes()[..2]), "");
        assert_eq!(s.finish(), "\u{fffd}");
        assert_eq!(s.finish(), "");
    }
}
