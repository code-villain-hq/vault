//! Getting the text out of what a model replied. Small models often wrap
//! the JSON in prose, repeat the prompt, or skip the JSON altogether, so this
//! tries several ways, as Glyph's Go server did.

use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

static TRANSLATION_OBJECT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\{\s*"translation"\s*:"#).unwrap());
static VARIANTS_OBJECT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\{\s*"variants"\s*:"#).unwrap());
static LABELLED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(?:^|\n)#{0,4}\s*Translation\s*:\s*\n?\s*"?([^"\n][^\n]*?)(?:"|$)"#).unwrap()
});

/// What a model is assumed to mean when it doesn't say how sure it is.
const UNSAID: f64 = 0.8;

/// The generated text in a llama-server reply: its own shape (`content`) or
/// an OpenAI-style one (`choices[0].text` or `.message.content`).
pub fn generated_text(reply: &Value) -> Option<&str> {
    for field in ["content", "response", "text"] {
        if let Some(s) = reply.get(field).and_then(Value::as_str) {
            return Some(s);
        }
    }
    let first = reply.get("choices")?.get(0)?;
    first
        .get("text")
        .and_then(Value::as_str)
        .or_else(|| first.get("message")?.get("content")?.as_str())
}

/// The first JSON value starting at `at`, ignoring what follows it.
fn value_at(text: &str, at: usize) -> Option<Value> {
    serde_json::Deserializer::from_str(&text[at..])
        .into_iter::<Value>()
        .next()?
        .ok()
}

/// A text the model copied from the prompt instead of translating.
fn is_placeholder(s: &str) -> bool {
    let s = s.trim().to_lowercase();
    s.is_empty()
        || s == "<translate>"
        || s == "hallo welt"
        || [
            "translated text here",
            "your translation here",
            "dein text hier",
            "corrected text here",
        ]
        .iter()
        .any(|p| s.contains(p))
}

/// A confidence from 0 to 1, rounded to two places; 95 means 0.95.
pub fn confidence(value: Option<&Value>) -> f64 {
    let mut c = value.and_then(Value::as_f64).unwrap_or(0.0);
    if c > 1.0 {
        c /= 100.0;
    }
    ((c.max(0.0) * 100.0).round() / 100.0).min(1.0)
}

/// A JSON string the model wrapped in quotes of its own: `"\"Hej\""`.
fn unwrap_quotes(s: &str) -> String {
    let s = s.trim();
    let s = s
        .strip_prefix("\\\"")
        .and_then(|s| s.strip_suffix("\\\""))
        .unwrap_or(s);
    for q in ['"', '\''] {
        if s.len() >= 2 && s.starts_with(q) && s.ends_with(q) {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

/// A line of free text, without the quotes and trailing junk models put
/// around it (`"Hej".`, `Hej" more`).
fn clean_line(s: &str) -> String {
    let mut s = s.trim().trim_matches('"');
    s = s.strip_prefix("\\\"").unwrap_or(s);
    s = s.strip_suffix("\\\"").unwrap_or(s);
    s = s.trim_matches('\'');
    if let Some(i) = s.find('"') {
        s = &s[..i];
    }
    s.trim_end_matches(['"', '\'']).trim().to_string()
}

/// The translation and how sure the model was.
pub fn translation(reply: &str) -> (String, f64) {
    // 1. The last {"translation": …} object that parses: models often echo
    // the example first.
    let last = TRANSLATION_OBJECT
        .find_iter(reply)
        .filter_map(|m| value_at(reply, m.start()))
        .filter_map(|v| {
            let text = v.get("translation")?.as_str()?;
            (!is_placeholder(text)).then(|| (unwrap_quotes(text), confidence(v.get("confidence"))))
        })
        .last();
    if let Some(found) = last {
        return found;
    }

    // 2. The outermost { … }.
    if let (Some(start), Some(end)) = (reply.find('{'), reply.rfind('}'))
        && start < end
        && let Ok(v) = serde_json::from_str::<Value>(&reply[start..=end])
        && let Some(text) = v.get("translation").and_then(Value::as_str)
        && !is_placeholder(text)
    {
        return (unwrap_quotes(text), confidence(v.get("confidence")));
    }

    // 3. "text | 0.95".
    for line in reply.lines() {
        let Some((text, sure)) = line.trim().rsplit_once('|') else {
            continue;
        };
        let text = clean_line(text);
        if is_placeholder(&text) {
            continue;
        }
        let sure = leading_number(sure.trim()).unwrap_or(UNSAID);
        return (text, confidence(Some(&Value::from(sure))));
    }

    // 4. "Translation: text".
    if let Some(found) = LABELLED.captures(reply) {
        let text = clean_line(&found[1]);
        if !is_placeholder(&text) {
            return (text, UNSAID);
        }
    }

    // 5. The first line that isn't a label or a scrap of JSON.
    const LABELS: &[&str] = &[
        "translate",
        "translation",
        "source",
        "json",
        "response",
        "###",
        "##",
        "#",
        "```",
        "---",
        "note:",
        "confidence",
    ];
    for line in reply.lines() {
        let text = clean_line(line);
        let lower = text.to_lowercase();
        if is_placeholder(&text)
            || LABELS.iter().any(|l| lower.starts_with(l))
            || text.starts_with('{')
            || text.starts_with('}')
        {
            continue;
        }
        return (text, UNSAID);
    }

    tracing::debug!(reply, "no translation found in the model's reply");
    (reply.trim().to_string(), 0.5)
}

/// The number a string starts with, like C's `%f`.
fn leading_number(s: &str) -> Option<f64> {
    let end = s
        .char_indices()
        .take_while(|(i, c)| c.is_ascii_digit() || *c == '.' || (*i == 0 && matches!(c, '-' | '+')))
        .map(|(i, c)| i + c.len_utf8())
        .last()?;
    s[..end].parse().ok()
}

/// The forms in a `{"variants": [...]}` reply, without blanks, repeats or
/// the term itself.
pub fn variants(reply: &str, term: &str) -> Vec<String> {
    let parse = |v: Value| -> Option<Vec<String>> {
        Some(
            v.get("variants")?
                .as_array()?
                .iter()
                .filter_map(|s| s.as_str().map(str::to_string))
                .collect(),
        )
    };
    let found = VARIANTS_OBJECT
        .find_iter(reply)
        .filter_map(|m| value_at(reply, m.start()).and_then(parse))
        .last()
        .or_else(|| {
            let (start, end) = (reply.find('{')?, reply.rfind('}')?);
            (start < end)
                .then(|| {
                    serde_json::from_str(&reply[start..=end])
                        .ok()
                        .and_then(parse)
                })
                .flatten()
        })
        .unwrap_or_default();

    let term = term.trim().to_lowercase();
    let mut seen = Vec::new();
    found
        .into_iter()
        .filter_map(|v| {
            let v = v.trim().to_string();
            let key = v.to_lowercase();
            if v.is_empty() || key == term || seen.contains(&key) {
                return None;
            }
            seen.push(key);
            Some(v)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn generated_text_in_either_shape() {
        assert_eq!(generated_text(&json!({ "content": "a" })), Some("a"));
        assert_eq!(
            generated_text(&json!({ "choices": [{ "text": "b" }] })),
            Some("b")
        );
        assert_eq!(
            generated_text(&json!({ "choices": [{ "message": { "content": "c" } }] })),
            Some("c")
        );
        assert_eq!(generated_text(&json!({ "nope": 1 })), None);
    }

    #[test]
    fn the_last_translation_object_wins() {
        let reply = r#"{"translation": "wrong", "confidence": 0.1}
Text to translate: "hello"
JSON Response:
{"translation": "{{count}} ok", "confidence": 0.95}
more noise {"translation": "final", "confidence": 0.88}"#;
        assert_eq!(translation(reply), ("final".into(), 0.88));
    }

    #[test]
    fn quotes_inside_a_translation_stay() {
        let reply = r#"{"translation": "Click \"Save\"", "confidence": 0.9}"#;
        assert_eq!(translation(reply).0, r#"Click "Save""#);
        let wrapped = r#"{"translation": "\"Hej\"", "confidence": 0.9}"#;
        assert_eq!(translation(wrapped).0, "Hej");
    }

    #[test]
    fn replies_without_json_still_count() {
        assert_eq!(
            translation("Hej världen | 0.9"),
            ("Hej världen".into(), 0.9)
        );
        assert_eq!(
            translation("### Translation:\n\"Hej\""),
            ("Hej".into(), UNSAID)
        );
        assert_eq!(
            translation("Translate:\n\"Hej då\"\n"),
            ("Hej då".into(), UNSAID)
        );
    }

    #[test]
    fn confidence_is_a_fraction() {
        assert_eq!(confidence(Some(&json!(95))), 0.95);
        assert_eq!(confidence(Some(&json!(-1))), 0.0);
        assert_eq!(confidence(Some(&json!(0.876))), 0.88);
        assert_eq!(confidence(None), 0.0);
    }

    #[test]
    fn variants_drop_the_term_and_repeats() {
        let reply = r#"Sure: {"variants": ["e-post", "e-posten", "E-posten", " ", "e-poster"]}"#;
        assert_eq!(variants(reply, "e-post"), ["e-posten", "e-poster"]);
        assert!(variants("no idea", "e-post").is_empty());
    }
}
