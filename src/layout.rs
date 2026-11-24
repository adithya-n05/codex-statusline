use unicode_linebreak::linebreaks;
use unicode_segmentation::UnicodeSegmentation;

/// Codex wraps text leaves at Unicode break opportunities and ASCII word hyphens.
/// Keep outer rows indivisible so the renderer clips them at each pane's own edge.
pub fn fixed_line(input: &str) -> String {
    let text = input.replace(|c: char| c.is_control() || c == ' ', "\u{a0}");
    let mut breaks = linebreaks(&text).map(|(offset, _)| offset).peekable();
    const LIMIT: usize = 8192;
    let mut out = String::with_capacity(text.len().min(LIMIT));
    let mut clip_end = 0;
    let mut after_hyphen = false;
    for (offset, cluster) in text.grapheme_indices(true) {
        while breaks.next_if(|next| *next < offset).is_some() {}
        let join = after_hyphen || breaks.peek() == Some(&offset);
        let extra = usize::from(join) * '\u{2060}'.len_utf8() + cluster.len();
        if out.len() <= LIMIT - '…'.len_utf8() {
            clip_end = out.len();
        }
        if out.len() + extra > LIMIT {
            out.truncate(clip_end);
            out.push('…');
            break;
        }
        if join {
            out.push('\u{2060}');
        }
        out.push_str(cluster);
        after_hyphen = cluster == "-";
    }
    out
}
/// Codex measures these three logical rows against the displayed pane's cell width.
pub fn footer(first: &str, middle: Vec<serde_json::Value>, last: &str) -> serde_json::Value {
    use serde_json::json;
    let mut children = vec![json!(fixed_line(first)), json!("\n")];
    children.extend(middle);
    children.extend([json!("\n"), json!(fixed_line(last))]);
    json!({"type":"text","children":children})
}
pub fn gauge(filled_percent: f64, pressure_percent: f64) -> serde_json::Value {
    let percent = filled_percent.clamp(0., 100.);
    let filled = if percent == 0. {
        0
    } else if percent == 100. {
        20
    } else {
        ((percent / 5.).round() as usize).clamp(1, 19)
    };
    let color = if pressure_percent >= 90.0 {
        "#E05252"
    } else if pressure_percent >= 75.0 {
        "#E78A53"
    } else if pressure_percent >= 50.0 {
        "#FBCB97"
    } else {
        "#5F8787"
    };
    let bar = format!("[{}{}]", "▮".repeat(filled), "·".repeat(20 - filled));
    serde_json::json!({"type":"text","props":{"color":color},"children":[bar]})
}
