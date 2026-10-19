//! Checks on a model's text: glossary terms used, and `{{ }}` placeholders
//! kept.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// A preferred term the model is told to use, with the other forms that
/// count as it (such as Swedish `e-posten` for `e-post`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GlossaryTerm {
    pub source_term: String,
    pub target_term: String,
    pub target_variants: Vec<String>,
}

/// A term the model's text left out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Violation {
    pub source_term: String,
    pub target_term: String,
}

/// The terms whose source term appears in `text`, ignoring case. Only these
/// go into the prompt, to keep it short.
pub fn terms_in<'a>(text: &str, glossary: &'a [GlossaryTerm]) -> Vec<&'a GlossaryTerm> {
    let lower = text.to_lowercase();
    glossary
        .iter()
        .filter(|g| {
            let term = g.source_term.trim();
            !term.is_empty() && lower.contains(&term.to_lowercase())
        })
        .collect()
}

/// Every term none of whose accepted forms appears in `translation` as a
/// word of its own. Lenient on case and punctuation; strict on compounds:
/// `e-postadressen` never counts as `e-post`.
pub fn glossary_violations(translation: &str, glossary: &[GlossaryTerm]) -> Vec<Violation> {
    if translation.is_empty() {
        return Vec::new();
    }
    let lower = translation.to_lowercase();
    glossary
        .iter()
        .filter(|g| {
            let forms = accepted_forms(g);
            // A rule with no forms at all would flag every text.
            !forms.is_empty() && !forms.iter().any(|f| contains_word(&lower, &f.to_lowercase()))
        })
        .map(|g| Violation {
            source_term: g.source_term.clone(),
            target_term: g.target_term.clone(),
        })
        .collect()
}

/// The term and its variants, trimmed, without blanks or repeats (ignoring
/// case), the term first.
pub fn accepted_forms(g: &GlossaryTerm) -> Vec<String> {
    let mut seen = Vec::<String>::new();
    let mut out = Vec::new();
    for form in std::iter::once(&g.target_term).chain(&g.target_variants) {
        let form = form.trim();
        if form.is_empty() || seen.contains(&form.to_lowercase()) {
            continue;
        }
        seen.push(form.to_lowercase());
        out.push(form.to_string());
    }
    out
}

/// Whether `needle` appears in `haystack` with a boundary on both sides. A
/// boundary is the end of the text or a byte that isn't an ASCII letter or
/// digit; every non-ASCII byte counts as a letter, so `e-post` isn't found
/// in `e-postå`.
fn contains_word(haystack: &str, needle: &str) -> bool {
    let (h, n) = (haystack.as_bytes(), needle.as_bytes());
    if n.is_empty() || n.len() > h.len() {
        return false;
    }
    let inside = |b: u8| b.is_ascii_alphanumeric() || b >= 0x80;
    (0..=h.len() - n.len()).any(|start| {
        let end = start + n.len();
        &h[start..end] == n
            && (start == 0 || !inside(h[start - 1]))
            && (end == h.len() || !inside(h[end]))
    })
}

static PLACEHOLDER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\{[\s\S]*?\}\}").unwrap());

/// How often each `{{ }}` placeholder appears.
pub fn placeholders(text: &str) -> BTreeMap<&str, usize> {
    let mut counts = BTreeMap::new();
    for m in PLACEHOLDER.find_iter(text) {
        *counts.entry(m.as_str()).or_default() += 1;
    }
    counts
}

/// Whether the translation keeps the source's placeholders: each as often
/// as in the source, and none at all when the source has none.
pub fn placeholders_kept(source: &str, translation: &str) -> bool {
    let src = placeholders(source);
    let tgt = placeholders(translation);
    if src.is_empty() {
        return tgt.is_empty();
    }
    src.iter()
        .all(|(token, n)| tgt.get(token).copied().unwrap_or(0) >= *n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn term(source: &str, target: &str, variants: &[&str]) -> GlossaryTerm {
        GlossaryTerm {
            source_term: source.into(),
            target_term: target.into(),
            target_variants: variants.iter().map(|v| v.to_string()).collect(),
        }
    }

    #[test]
    fn glossary_terms_must_appear_as_words() {
        let email = [term("email", "e-post", &[])];
        let variants = [term("email", "e-post", &["e-posten", "e-poster", "e-posterna"])];
        let both = [term("email", "e-post", &[]), term("password", "lösenord", &[])];
        let cases: &[(&str, &[GlossaryTerm], usize)] = &[
            ("Har du glömt din e-post?", &email, 0),
            ("E-post address is required.", &email, 0),
            ("Den här e-postadressen är redan registrerad.", &email, 1),
            ("e-postmeddelande skickat.", &email, 1),
            ("E-POST är bra", &email, 0),
            ("Skicka e-post.", &email, 0),
            ("e-postå", &email, 1),
            ("Skicka meddelande.", &email, 1),
            ("Logga in med e-post och lösen.", &both, 1),
            ("anything", &[], 0),
            ("E-posten är redan registrerad.", &variants, 0),
            ("E-postadressen eller lösenordet är felaktigt.", &variants, 1),
            ("Kolla e-posterna nu.", &variants, 0),
        ];
        for (text, glossary, want) in cases {
            assert_eq!(glossary_violations(text, glossary).len(), *want, "{text}");
        }
    }

    #[test]
    fn only_terms_in_the_text_go_to_the_model() {
        let glossary = [term("email", "e-post", &[]), term("password", "lösenord", &[])];
        let found = terms_in("Your Email address", &glossary);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].source_term, "email");
    }

    #[test]
    fn accepted_forms_drop_blanks_and_repeats() {
        let g = term("email", "e-post", &[" E-post ", "", "e-posten"]);
        assert_eq!(accepted_forms(&g), ["e-post", "e-posten"]);
    }

    #[test]
    fn placeholders_must_survive() {
        assert!(placeholders_kept("Hi {{name}}", "Hej {{name}}"));
        assert!(!placeholders_kept("{{a}} {{a}}", "{{a}}"));
        assert!(!placeholders_kept("Hello {{name}} there", "Hello there"));
        assert!(placeholders_kept("no placeholders", "inget"));
        assert!(!placeholders_kept("no placeholders", "{{count}} {{name}}s"));
    }
}
