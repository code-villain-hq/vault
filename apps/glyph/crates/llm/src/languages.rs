/// A language's English name for the prompt, such as "Swedish" for `sv`.
/// Models translate better to "Swedish" than to "sv"; codes without a name
/// here go as they are. A region or script follows the name: `pt-BR` is
/// "Portuguese (BR)".
pub fn name(code: &str) -> String {
    let (primary, rest) = code.split_once('-').unwrap_or((code, ""));
    let name = match primary.to_ascii_lowercase().as_str() {
        "ar" => "Arabic",
        "cs" => "Czech",
        "da" | "dk" => "Danish",
        "de" => "German",
        "el" => "Greek",
        "en" => "English",
        "es" => "Spanish",
        "et" => "Estonian",
        "fi" => "Finnish",
        "fr" => "French",
        "hu" => "Hungarian",
        "is" => "Icelandic",
        "it" => "Italian",
        "ja" => "Japanese",
        "ko" => "Korean",
        "lt" => "Lithuanian",
        "lv" => "Latvian",
        "nb" => "Norwegian Bokmål",
        "nl" => "Dutch",
        "nn" => "Norwegian Nynorsk",
        "no" => "Norwegian",
        "pl" => "Polish",
        "pt" => "Portuguese",
        "ro" => "Romanian",
        "ru" => "Russian",
        "sk" => "Slovak",
        "sv" => "Swedish",
        "tr" => "Turkish",
        "uk" => "Ukrainian",
        "zh" => "Chinese",
        _ => return code.to_string(),
    };
    if rest.is_empty() {
        name.to_string()
    } else {
        format!("{name} ({rest})")
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn codes_become_names() {
        assert_eq!(super::name("sv"), "Swedish");
        assert_eq!(super::name("pt-BR"), "Portuguese (BR)");
        assert_eq!(super::name("xx"), "xx");
    }
}
