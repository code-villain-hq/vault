//! The start of a mail's text as the list shows it: one line of words,
//! without what a text part carries for its links and layout. Link
//! addresses go and their words stay; the `[1]` that numbers a link,
//! pictures written in, rules, HTML comments and the invisible padding
//! newsletters put after their first line go too.

/// How long a preview is, in characters.
const LENGTH: usize = 160;

/// The list's line for a mail's text.
pub fn preview(text: &str) -> String {
    let text = uncomment(text);
    let mut text: String = text.chars().filter(|c| !is_invisible(*c)).collect();
    // A link's words can hold a link of their own: a picture in a link.
    for _ in 0..3 {
        let unlinked = unlink(&text);
        if unlinked == text {
            break;
        }
        text = unlinked;
    }
    unnumber(&text)
        .split_whitespace()
        .filter_map(word)
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(LENGTH)
        .collect()
}

/// The text without `<!-- … -->`; one left open hides the rest.
fn uncomment(text: &str) -> String {
    let mut kept = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<!--") {
        kept.push_str(&rest[..start]);
        kept.push(' ');
        match rest[start..].find("-->") {
            Some(end) => rest = &rest[start + end + 3..],
            None => return kept,
        }
    }
    kept.push_str(rest);
    kept
}

/// `[words](address)` as its words, and `![words](address)` too.
fn unlink(text: &str) -> String {
    let mut kept = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(close) = rest.find("](") {
        let open = rest[..close].rfind('[');
        let end = rest[close + 2..].find(')').map(|end| close + 2 + end);
        let (Some(open), Some(end)) = (open, end) else {
            kept.push_str(&rest[..close + 2]);
            rest = &rest[close + 2..];
            continue;
        };
        let before = rest[..open].strip_suffix('!').unwrap_or(&rest[..open]);
        kept.push_str(before);
        kept.push_str(&rest[open + 1..close]);
        rest = &rest[end + 1..];
    }
    kept.push_str(rest);
    kept
}

/// The text without the `[1]` a text part numbers a link with, its
/// address listed at the end: "online-versionen [1]" reads as
/// "online-versionen".
fn unnumber(text: &str) -> String {
    let mut kept = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        kept.push_str(&rest[..open]);
        let digits = rest[open + 1..]
            .bytes()
            .take_while(u8::is_ascii_digit)
            .count();
        if digits > 0 && rest[open + 1 + digits..].starts_with(']') {
            rest = &rest[open + digits + 2..];
        } else {
            kept.push('[');
            rest = &rest[open + 1..];
        }
    }
    kept.push_str(rest);
    kept
}

/// A word of the line, or `None` for an address, a picture, a rule or a
/// mark that only lays text out. `**Bold**` reads as "Bold".
fn word(word: &str) -> Option<&str> {
    let bare = word.trim_start_matches(['(', '<', '[', '"', '\'']);
    let lower = bare.to_ascii_lowercase();
    if word.contains("://")
        || ["www.", "data:", "cid:", "mailto:"]
            .iter()
            .any(|start| lower.starts_with(start))
    {
        return None;
    }
    if word.chars().all(|c| "#*|>•·".contains(c))
        || (word.chars().count() >= 3 && word.chars().all(|c| "-_=~+.".contains(c)))
    {
        return None;
    }
    if ["[]", "()", "<>", "[ ]"].contains(&word) {
        return None;
    }
    let unmarked = word.trim_matches(['*', '_']);
    Some(if unmarked.is_empty() { word } else { unmarked })
}

/// Characters that take no room: what newsletters pad their first line
/// with, so a mail app shows nothing more of the text.
fn is_invisible(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}' | '\u{034F}' | '\u{180E}' | '\u{200B}'..='\u{200F}' | '\u{2060}'..='\u{2064}' | '\u{FEFF}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_reads_as_its_words() {
        assert_eq!(
            preview("See [the new release](https://example.test/r?id=1) now."),
            "See the new release now."
        );
        assert_eq!(
            preview("*[View in browser](https://example.test/v)* Hello"),
            "View in browser Hello"
        );
        assert_eq!(
            preview("[![Logo](https://example.test/l.png)](https://example.test) Hi"),
            "Logo Hi"
        );
    }

    #[test]
    fn a_link_numbered_in_the_text_leaves_no_number() {
        assert_eq!(
            preview(
                "Till online-versionen [1] [1] [1] Kundnummer: 4711\n\n[1] https://example.test/v"
            ),
            "Till online-versionen Kundnummer: 4711"
        );
        assert_eq!(
            preview("See the terms[12], and [x] stays [ 3 ]."),
            "See the terms, and [x] stays [ 3 ]."
        );
    }

    #[test]
    fn addresses_pictures_and_rules_go() {
        assert_eq!(
            preview(
                "Your order\n------\nhttps://shop.example.test/orders/123456789 \
                 data:image/png;base64,iVBORw0KGgo= is on its way. (www.example.test)"
            ),
            "Your order is on its way."
        );
        assert_eq!(
            preview("Home | Shop | About\n===\n**Sale** ends today"),
            "Home Shop About Sale ends today"
        );
    }

    #[test]
    fn comments_and_padding_go() {
        assert_eq!(
            preview(
                "<!-- preheader -->Your week in review\u{034F} \u{200C} \u{034F} \u{200C} Read on"
            ),
            "Your week in review Read on"
        );
        assert_eq!(preview("Hello <!-- never closed"), "Hello");
    }

    #[test]
    fn plain_text_stays_as_it_is() {
        assert_eq!(
            preview("Hi Michael,\r\n\r\nthe contract is attached - see page 3_a."),
            "Hi Michael, the contract is attached - see page 3_a."
        );
    }

    #[test]
    fn a_preview_is_one_short_line() {
        assert_eq!(preview(&"word ".repeat(100)).chars().count(), LENGTH);
    }
}
