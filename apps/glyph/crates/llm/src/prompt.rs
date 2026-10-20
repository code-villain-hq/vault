//! The prompts, as in Glyph's Go server. In chat mode the system message
//! asks for JSON; in completion mode each prompt asks for it itself.

use std::fmt::Write;

use crate::check::{GlossaryTerm, Violation, accepted_forms, placeholders};

/// The system message in chat mode.
pub const SYSTEM: &str = "You are a translator. Translate the user's text. Output ONLY JSON: \
{\"translation\": \"...\", \"confidence\": 0.95}. No other text.";

const PLACEHOLDER_RULES: &str = r#"
Template placeholders — CRITICAL (apply only to tokens that appear in the SOURCE string below, not to examples elsewhere in this prompt):
  * If the source contains Mustache-style tokens (segments starting with "{{" and ending with the next "}}"), copy each one into your translation EXACTLY as in the source: same characters and spacing inside the braces. Do NOT translate or rename anything between {{ and }}.
  * You may move a whole token elsewhere in the sentence for grammar, but each token string must remain byte-identical to how it appears in the source.
  * If the source has NO such tokens, your translation must have NONE — do not invent "{{...}}" from instructions, JSON examples, or glossaries.
  * Do not drop or merge tokens that exist in the source."#;

const JSON_REPLY: &str = "\n\nReply ONLY with a JSON object with two fields: \"translation\" (the corrected text) and \"confidence\" (a number between 0 and 1).\n\nJSON:";

/// What to translate, and the rules for it.
pub struct Job<'a> {
    /// Language names, such as "English".
    pub source_lang: &'a str,
    pub target_lang: &'a str,
    pub text: &'a str,
    /// The glossary terms that appear in the text.
    pub glossary: &'a [GlossaryTerm],
    /// Where the text appears in the product, for tone.
    pub context: &'a str,
}

/// A string in double quotes, escaped as JSON.
fn quote(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_default()
}

fn forms_list(g: &GlossaryTerm) -> String {
    accepted_forms(g)
        .iter()
        .map(|f| quote(f))
        .collect::<Vec<_>>()
        .join(", ")
}

fn mapping(g: &GlossaryTerm) -> String {
    if g.target_variants.is_empty() {
        format!(
            "  - {} → {}\n",
            quote(&g.source_term),
            quote(&g.target_term)
        )
    } else {
        format!(
            "  - {} → {} (accepted forms: {})\n",
            quote(&g.source_term),
            quote(&g.target_term),
            forms_list(g)
        )
    }
}

fn context_block(context: &str) -> String {
    let context = context.trim();
    if context.is_empty() {
        return String::new();
    }
    format!(
        "\nWhere this string appears in the product (use for tone, register, and disambiguation — do not translate this paragraph literally into the output):\n{context}\n"
    )
}

fn glossary_block(glossary: &[GlossaryTerm]) -> String {
    if glossary.is_empty() {
        return String::new();
    }
    let mut s = String::from(
        "\nGlossary — MANDATORY preferred terms. Each source term MUST be rendered as one of the accepted forms listed below.\n\
         Rules:\n\
         \x20 * Use ONLY the accepted forms listed for each mapping. When a list of accepted forms is given, any one of them is fine — pick whichever fits the sentence grammar.\n\
         \x20 * Do NOT embed the target term inside a different compound word or add a foreign prefix/suffix (e.g. in Swedish, output \"e-post\" or \"e-posten\", never \"e-postadressen\" or \"e-postmeddelande\").\n\
         \x20 * If a rewording is needed to fit the sentence, reword the rest of the sentence — never the glossary term.\n\
         Mappings:\n",
    );
    for g in glossary {
        s.push_str(&mapping(g));
    }
    s
}

/// The first prompt. Unlike the Go server, chat mode gets the glossary,
/// placeholder rules and context too: without them the model never saw the
/// glossary it was checked against.
pub fn translate(job: &Job, chat: bool) -> String {
    let rules = format!(
        "{PLACEHOLDER_RULES}{}{}",
        context_block(job.context),
        glossary_block(job.glossary)
    );
    if chat {
        return format!(
            "Translate from {} to {}.\n{rules}\nText to translate: {}",
            job.source_lang,
            job.target_lang,
            quote(job.text)
        );
    }
    format!(
        "You are a translation engine. Translate the following text from {} to {}.\n\
         Reply ONLY with a JSON object with two fields: \"translation\" (the translated text) and \"confidence\" (a number between 0 and 1).\n\
         {rules}\n\
         Source text: {}\n\nJSON:",
        job.source_lang,
        job.target_lang,
        quote(job.text)
    )
}

/// The retry when the model's text dropped, changed or invented a
/// placeholder. Placeholders come before the glossary: a broken one breaks
/// the app in every language.
pub fn fix_placeholders(job: &Job, previous: &str, chat: bool) -> String {
    let src = placeholders(job.text);
    let tgt = placeholders(previous);
    let note = match job.context.trim() {
        "" => String::new(),
        c => format!("\nProduct / UI context (keep in mind while fixing placeholders):\n{c}\n"),
    };
    let suffix = if chat {
        "\n\nProvide the corrected translation."
    } else {
        JSON_REPLY
    };
    let (from, to) = (job.source_lang, job.target_lang);
    let (text, previous) = (quote(job.text), quote(previous));

    if src.is_empty() && !tgt.is_empty() {
        let invented = tgt.keys().copied().collect::<Vec<_>>().join(", ");
        return format!(
            "You are a professional translator. Your previous translation from {from} to {to} is INVALID.\n\n\
             The source text contains NO Mustache-style \"{{{{...}}}}\" template tokens — only plain words and punctuation.\n\
             Your translation incorrectly invented template tokens (remove ALL of them; the output must be natural language only): {invented}\n\n\
             Source text: {text}\n\
             Your previous (INCORRECT) translation: {previous}\n\
             {note}\n\
             Rewrite the translation into the target language with ZERO \"{{{{\" or \"}}}}\" template syntax.{suffix}"
        );
    }

    let problems: Vec<String> = src
        .iter()
        .filter_map(|(token, want)| {
            let have = tgt.get(token).copied().unwrap_or(0);
            (have < *want).then(|| {
                format!(
                    "  * {} must appear exactly {want} time(s) in the translation (found {have}).",
                    quote(token)
                )
            })
        })
        .collect();
    format!(
        "You are a professional translator. Your previous translation from {from} to {to} broke CRITICAL rules about template placeholders.\n\n\
         Source text: {text}\n\
         Your previous (INCORRECT) translation: {previous}\n\
         {note}\n\
         The following placeholder tokens from the source are missing or were altered (each {{{{ ... }}}} block must appear verbatim, same number of times as in the source):\n\
         {}\n\n\
         Rewrite the translation so EVERY \"{{{{ ... }}}}\" token from the source appears in your output EXACTLY unchanged — same text inside the braces, same braces. Translate only the natural-language parts outside these tokens.{suffix}",
        problems.join("\n")
    )
}

/// The retry when the model's text left out glossary terms: its own text
/// back, with the rules it broke. Models do better at proofreading than at
/// trying again from scratch.
pub fn fix_glossary(job: &Job, previous: &str, violations: &[Violation], chat: bool) -> String {
    let forms_of = |v: &Violation| {
        job.glossary
            .iter()
            .find(|g| g.source_term == v.source_term)
            .map(accepted_forms)
            .unwrap_or_else(|| vec![v.target_term.clone()])
    };
    let listed = |forms: &[String]| {
        forms
            .iter()
            .map(|f| quote(f))
            .collect::<Vec<_>>()
            .join(", ")
    };

    let mut s = format!(
        "You are a professional translator. Your previous translation from {} to {} broke glossary rules and must be corrected.\n\n\
         Preserve every {{{{ ... }}}} placeholder from the source text EXACTLY (byte-identical) in your corrected output — translate only the natural-language parts outside those tokens.\n\n",
        job.source_lang, job.target_lang
    );
    let context = context_block(job.context);
    if !context.is_empty() {
        s.push_str(context.trim());
        s.push_str("\n\n");
    }
    let _ = write!(
        s,
        "Source text: {}\nYour previous (INCORRECT) translation: {}\n\nThe previous translation is INCORRECT because:\n",
        quote(job.text),
        quote(previous)
    );
    for v in violations {
        let forms = forms_of(v);
        if forms.len() <= 1 {
            let _ = writeln!(
                s,
                "  * {} must be translated as EXACTLY {} — not inside a compound word, not inflected, not pluralized, not with any prefix or suffix.",
                quote(&v.source_term),
                quote(&v.target_term)
            );
        } else {
            let _ = writeln!(
                s,
                "  * {} must be translated using one of these accepted forms: {}. Do NOT embed it in a different compound word (e.g. no \"-adressen\" suffix) and do NOT invent new inflections.",
                quote(&v.source_term),
                listed(&forms)
            );
        }
    }
    s.push_str("\nRewrite the translation so every glossary term above appears as one of its accepted forms, as a standalone word. If the sentence structure needs to change to make that work, change it. Do not include any explanation.\n\n");
    if !job.glossary.is_empty() {
        s.push_str("Full glossary (use one of the accepted forms for each mapping):\n");
        for g in job.glossary {
            s.push_str(&mapping(g));
        }
        s.push('\n');
    }
    s.push_str("Correct the translation by following these rules:\n");
    for v in violations {
        let forms = forms_of(v);
        if forms.len() <= 1 {
            let _ = writeln!(
                s,
                "  - {} MUST be {} (not in a compound word)",
                quote(&v.source_term),
                quote(&v.target_term)
            );
        } else {
            let _ = writeln!(
                s,
                "  - {} MUST use one of: {}",
                quote(&v.source_term),
                listed(&forms)
            );
        }
    }
    if chat {
        s.push_str("\nProvide the corrected translation.");
    } else {
        s.push_str(JSON_REPLY.trim_start_matches('\n'));
    }
    s
}

/// Asks for a term's grammatical forms: inflections, never synonyms or
/// compounds.
pub fn variants(
    source_lang: &str,
    target_lang: &str,
    source_term: &str,
    target_term: &str,
) -> String {
    format!(
        r#"You are a {target_lang} linguist. The word "{target_term}" is the canonical {target_lang} translation of the {source_lang} term "{source_term}".

List every common grammatical form of "{target_term}" that a reviewer should accept as equivalent in translations — e.g. definite, plural, genitive, or case-marked forms.

STRICT RULES:
  * ONLY grammatical inflections of "{target_term}". No synonyms.
  * NEVER embed "{target_term}" inside a different compound word (e.g. in Swedish do NOT include "e-postadressen" or "e-postmeddelande" as forms of "e-post").
  * If the term contains Mustache-style "{{{{" "}}}}" segments, copy those segments verbatim in every variant — never translate or edit text inside double braces.
  * Include the canonical form "{target_term}" itself if it is a normal surface form.
  * Return at most 6 forms. If the language has little or no inflection, return fewer.

Respond ONLY with valid JSON in this exact format:
{{
  "variants": ["form1", "form2", "form3"]
}}

JSON Response:"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job<'a>(text: &'a str, glossary: &'a [GlossaryTerm]) -> Job<'a> {
        Job {
            source_lang: "English",
            target_lang: "Swedish",
            text,
            glossary,
            context: "",
        }
    }

    #[test]
    fn the_glossary_reaches_the_model_in_both_modes() {
        let glossary = [GlossaryTerm {
            source_term: "email".into(),
            target_term: "e-post".into(),
            target_variants: vec!["e-posten".into()],
        }];
        for chat in [true, false] {
            let prompt = translate(&job("Your email", &glossary), chat);
            assert!(
                prompt.contains(r#""email" → "e-post" (accepted forms: "e-post", "e-posten")"#)
            );
            assert!(prompt.contains(r#""Your email""#));
        }
    }

    #[test]
    fn a_placeholder_fix_names_what_went_missing() {
        let prompt = fix_placeholders(&job("Hi {{name}}", &[]), "Hej", false);
        assert!(
            prompt.contains(
                r#""{{name}}" must appear exactly 1 time(s) in the translation (found 0)."#
            )
        );
        let invented = fix_placeholders(&job("Hi", &[]), "Hej {{x}}", true);
        assert!(invented.contains("invented template tokens"));
        assert!(invented.contains("{{x}}"));
    }
}
