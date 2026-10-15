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

/// A colour as `#rrggbb`.
pub fn color(value: &str) -> Result<(), ValidationError> {
    let valid = value.len() == 7
        && value.starts_with('#')
        && value[1..].chars().all(|c| c.is_ascii_hexdigit());
    if valid {
        Ok(())
    } else {
        invalid("colour must look like #a1b2c3")
    }
}

/// 0 = none, 1 = urgent, 2 = high, 3 = medium, 4 = low.
pub fn priority(value: i16) -> Result<(), ValidationError> {
    if (0..=4).contains(&value) {
        Ok(())
    } else {
        invalid("priority must be between 0 and 4")
    }
}

/// Runs `check` on a value if present.
pub fn optional<T>(
    value: &Option<T>,
    check: impl FnOnce(&T) -> Result<(), ValidationError>,
) -> Result<(), ValidationError> {
    value.as_ref().map(check).transpose().map(|_| ())
}

pub const NAME: usize = 200;
pub const TITLE: usize = 500;
pub const DESCRIPTION: usize = 100_000;
