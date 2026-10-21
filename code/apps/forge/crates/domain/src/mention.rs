//! Mentions: `@Ada Lovelace`, or `@Ada` when only one person has that first
//! name, in comments and issue descriptions. Plain text, so people and
//! agents write them the same way.

use uuid::Uuid;

/// The people mentioned in `text`, each once, in the order first mentioned.
/// `people` are the candidates: (id, full name).
pub fn mentions(text: &str, people: &[(Uuid, String)]) -> Vec<Uuid> {
    // Every name someone can be mentioned by, longest first, so "@Ada
    // Lovelace" isn't taken for "@Ada" when both exist.
    let mut names: Vec<(String, Uuid)> = Vec::new();
    for (id, name) in people {
        let full = name.trim().to_lowercase();
        if full.is_empty() {
            continue;
        }
        if let Some(first) = full.split_whitespace().next().filter(|f| *f != full) {
            let shared = people.iter().any(|(other, n)| {
                other != id && n.trim().to_lowercase().split_whitespace().next() == Some(first)
            });
            if !shared {
                names.push((first.to_string(), *id));
            }
        }
        names.push((full, *id));
    }
    names.sort_by_key(|(name, _)| std::cmp::Reverse(name.chars().count()));

    let lower = text.to_lowercase();
    let mut found = Vec::new();
    for (at, _) in lower.match_indices('@') {
        // Not an email address: the @ starts a word.
        if lower[..at]
            .chars()
            .next_back()
            .is_some_and(char::is_alphanumeric)
        {
            continue;
        }
        let rest = &lower[at + 1..];
        let matched = names.iter().find(|(name, _)| {
            rest.starts_with(name.as_str())
                && !rest[name.len()..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_')
        });
        if let Some((_, id)) = matched
            && !found.contains(id)
        {
            found.push(*id);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADA: Uuid = Uuid::from_u128(1);
    const ADAM: Uuid = Uuid::from_u128(2);
    const BOB: Uuid = Uuid::from_u128(3);
    const BOBBY: Uuid = Uuid::from_u128(4);

    fn people() -> Vec<(Uuid, String)> {
        vec![
            (ADA, "Ada Lovelace".into()),
            (ADAM, "Adam Smith".into()),
            (BOB, "Bob Stone".into()),
            (BOBBY, "Bob Marley".into()),
        ]
    }

    #[test]
    fn full_names_and_unique_first_names() {
        assert_eq!(
            mentions("@ada lovelace and @Adam, please look", &people()),
            [ADA, ADAM]
        );
        assert_eq!(mentions("thanks @Ada!", &people()), [ADA]);
    }

    #[test]
    fn shared_first_names_need_the_full_name() {
        assert!(mentions("@Bob can you check", &people()).is_empty());
        assert_eq!(mentions("@Bob Marley can you check", &people()), [BOBBY]);
    }

    #[test]
    fn not_emails_partial_words_or_repeats() {
        assert!(mentions("mail ada@lovelace.dev", &people()).is_empty());
        assert!(mentions("@Adaline", &people()).is_empty());
        assert_eq!(mentions("@Ada @Ada Lovelace @ada", &people()), [ADA]);
    }

    #[test]
    fn agents_named_after_where_they_run() {
        const RUNS: Uuid = Uuid::from_u128(5);
        const CHAT: Uuid = Uuid::from_u128(6);
        let agents = vec![
            (RUNS, "Claude Code".into()),
            (CHAT, "Claude Code (chat)".into()),
        ];
        assert_eq!(mentions("@Claude Code (chat) did this", &agents), [CHAT]);
        assert_eq!(mentions("@Claude Code, yours", &agents), [RUNS]);
        assert!(mentions("@Claude look", &agents).is_empty());
    }
}
