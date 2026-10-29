/// Joins decoded segments into one line, dropping whisper's non-speech markers
/// (`[BLANK_AUDIO]`, `[ Silence ]`, ...) so silence does not reach the transcript panel.
pub fn join_segments<'a>(segments: impl IntoIterator<Item = &'a str>) -> String {
    let mut out = String::new();
    for seg in segments {
        let seg = seg.trim();
        if seg.is_empty() || is_non_speech_marker(seg) {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(seg);
    }
    out
}

fn is_non_speech_marker(seg: &str) -> bool {
    seg.starts_with('[') && seg.ends_with(']')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_segments_trims_and_joins() {
        assert_eq!(
            join_segments([" Xin chào ", "các bạn."]),
            "Xin chào các bạn."
        );
    }

    #[test]
    fn join_segments_drops_non_speech_markers() {
        assert_eq!(join_segments([" [BLANK_AUDIO]"]), "");
        assert_eq!(join_segments(["[ Silence ]", "Hello", ""]), "Hello");
    }
}
