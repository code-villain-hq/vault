//! Small checks shared by the models' validation.

use crate::model::ValidationError;

pub fn invalid<T>(message: impl Into<String>) -> Result<T, ValidationError> {
    Err(ValidationError(message.into()))
}

/// Non-blank and at most `max` characters.
pub fn text(value: &str, what: &str, max: usize) -> Result<(), ValidationError> {
    if value.trim().is_empty() {
        return invalid(format!("{what} can't be empty"));
    }
    long_text(value, what, max)
}

/// At most `max` characters; may be empty.
pub fn long_text(value: &str, what: &str, max: usize) -> Result<(), ValidationError> {
    if value.chars().count() > max {
        return invalid(format!("{what} is longer than {max} characters"));
    }
    Ok(())
}

/// A language code such as `en`, `sv` or `pt-BR`: two or three letters,
/// then optional parts of letters and digits after dashes.
pub fn language_code(value: &str) -> Result<(), ValidationError> {
    let mut parts = value.split('-');
    let primary = parts.next().unwrap_or_default();
    let valid = (2..=3).contains(&primary.len())
        && primary.chars().all(|c| c.is_ascii_alphabetic())
        && parts
            .all(|p| (1..=8).contains(&p.len()) && p.chars().all(|c| c.is_ascii_alphanumeric()))
        && value.len() <= 35;
    if valid {
        Ok(())
    } else {
        invalid(format!(
            "{value:?} isn't a language code like en, sv or pt-BR"
        ))
    }
}

/// A namespace name, which is also a file name in exports: no slashes, no
/// colons (they separate namespace and key on import) and no dots at the start.
pub fn namespace(value: &str) -> Result<(), ValidationError> {
    text(value, "namespace", NAME)?;
    if value.trim() != value {
        return invalid("a namespace can't start or end with a space");
    }
    if value.contains(['/', '\\', ':']) || value.starts_with('.') {
        return invalid("a namespace can't contain /, \\ or :, or start with a dot");
    }
    Ok(())
}

pub fn key(value: &str) -> Result<(), ValidationError> {
    text(value, "key", KEY)?;
    if value.trim() != value {
        return invalid("a key can't start or end with a space");
    }
    Ok(())
}

/// A fraction from 0 to 1.
pub fn unit(value: f64, what: &str) -> Result<(), ValidationError> {
    if (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        invalid(format!("{what} must be between 0 and 1"))
    }
}

/// Runs `check` on a value if present.
pub fn optional<T>(
    value: &Option<T>,
    check: impl FnOnce(&T) -> Result<(), ValidationError>,
) -> Result<(), ValidationError> {
    value.as_ref().map(check).transpose().map(|_| ())
}

pub const NAME: usize = 100;
pub const KEY: usize = 500;
pub const TERM: usize = 200;
pub const NOTE: usize = 2_000;
pub const VALUE: usize = 20_000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_codes_look_like_bcp47() {
        for good in ["en", "sv", "fil", "pt-BR", "zh-Hant-TW"] {
            assert!(language_code(good).is_ok(), "{good}");
        }
        for bad in ["", "e", "english", "en_", "en-", "pt_BR", "1a", "en--x"] {
            assert!(language_code(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn namespaces_are_file_names() {
        assert!(namespace("common").is_ok());
        assert!(namespace("billing-v2").is_ok());
        for bad in ["", " common", "a/b", "a:b", ".hidden"] {
            assert!(namespace(bad).is_err(), "{bad}");
        }
    }
}
