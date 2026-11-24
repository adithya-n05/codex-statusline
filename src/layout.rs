use unicode_linebreak::linebreaks;
use unicode_segmentation::UnicodeSegmentation;

/// Codex wraps text leaves at Unicode break opportunities and ASCII word hyphens.
/// Keep outer rows indivisible so the renderer clips them at each pane's own edge.
pub fn fixed_line(input: &str) -> String {
    let text = input.replace(|c: char| c.is_control() || c == ' ', "\u{a0}");
    let mut breaks = linebreaks(&text).map(|(offset, _)| offset).peekable();
    let mut out = String::with_capacity(text.len());
    let mut after_hyphen = false;
    for (offset, cluster) in text.grapheme_indices(true) {
        while breaks.next_if(|next| *next < offset).is_some() {}
        if after_hyphen || breaks.peek() == Some(&offset) {
            out.push('\u{2060}');
        }
        out.push_str(cluster);
        after_hyphen = cluster == "-";
    }
    out
}
