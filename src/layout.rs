use unicode_linebreak::linebreaks;
use unicode_segmentation::UnicodeSegmentation;

const LIMIT: usize = 8192;
const DIVIDER_COLOR: &str = "#969696";

/// Codex wraps text leaves at Unicode break opportunities and ASCII word hyphens.
/// Keep outer rows indivisible so the renderer clips them at each pane's own edge.
pub fn fixed_line(input: &str) -> String {
    let text = input.replace(|c: char| c.is_control() || c == ' ', "\u{a0}");
    let mut breaks = linebreaks(&text).map(|(offset, _)| offset).peekable();
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
pub(crate) fn header(fields: &[(&str, &str)]) -> serde_json::Value {
    use serde_json::json;
    let mut parts = Vec::new();
    for (i, &(text, color)) in fields.iter().enumerate() {
        if i != 0 {
            parts.push(("\u{a0}·\u{a0}".to_owned(), DIVIDER_COLOR));
        }
        parts.push((fixed_line(text), color));
    }
    let length: usize = parts.iter().map(|(text, _)| text.len()).sum();
    let mut room = if length > LIMIT {
        LIMIT - '…'.len_utf8()
    } else {
        LIMIT
    };
    let mut spans = Vec::new();
    for (mut text, color) in parts {
        if text.len() > room {
            let end = text
                .grapheme_indices(true)
                .map(|(i, _)| i)
                .take_while(|&i| i <= room)
                .last()
                .unwrap_or(0);
            text.truncate(end);
            text.push('…');
            spans.push(painted(text, color));
            break;
        }
        room -= text.len();
        spans.push(painted(text, color));
    }
    json!({"type":"text","children":spans})
}

/// Codex measures these three logical rows against the displayed pane's cell width.
pub fn footer(first: &str, middle: Vec<serde_json::Value>, last: &str) -> serde_json::Value {
    assemble_footer(serde_json::json!(fixed_line(first)), middle, last)
}
pub(crate) fn assemble_footer(
    first: serde_json::Value,
    middle: Vec<serde_json::Value>,
    last: &str,
) -> serde_json::Value {
    use serde_json::json;
    let mut children = vec![first, json!("\n")];
    children.extend(middle);
    children.extend([json!("\n"), activity_line(last)]);
    json!({"type":"text","props":{"color":DIVIDER_COLOR},"children":children})
}
fn activity_line(last: &str) -> serde_json::Value {
    use serde_json::json;
    const GAP: &str = "\u{a0}·\u{a0}";
    let line = fixed_line(last);
    let mut spans = Vec::new();
    for (index, field) in line.split(GAP).enumerate() {
        if index > 0 {
            spans.push(json!(GAP));
        }
        match field.rsplit_once('\u{a0}') {
            Some(("Tools", _)) => spans.push(painted(field, "#E78A53")),
            Some(("Failed", _)) => spans.push(painted(field, "#CC7373")),
            Some((label, count)) if label != "CLI" && count.parse::<usize>().is_ok() => {
                let accent = if label == "MCP" { "#E78A53" } else { "#C1C1C1" };
                spans.push(painted(label, accent));
                spans.push(json!("\u{a0}"));
                spans.push(painted(count, "#FBCB97"));
            }
            _ => spans.push(json!(field)),
        }
    }
    json!({"type":"text","props":{"color":DIVIDER_COLOR},"children":spans})
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
    let bar = format!("[{}{}]", "█".repeat(filled), "░".repeat(20 - filled));
    serde_json::json!({"type":"text","props":{"color":color},"children":[bar]})
}
pub fn painted(text: impl Into<String>, color: &str) -> serde_json::Value {
    serde_json::json!({"type":"text","props":{"color":color},"children":[text.into()]})
}
pub fn paint_gauge_labels(parts: &mut [serde_json::Value]) {
    let [label, bar, details @ ..] = parts else {
        return;
    };
    let Some(hue) = bar
        .pointer("/props/color")
        .and_then(serde_json::Value::as_str)
    else {
        return;
    };
    if let Some(text) = label.as_str() {
        *label = painted(text, hue);
    }
    for detail in details {
        if let Some(text) = detail.as_str() {
            *detail = painted(text, "#969696");
        }
    }
}
