//! The questions being answered, by their tokens. Each question opens one
//! ([`Questions::open`]), which Claude Code sends with every call to the
//! app's MCP endpoint, and the asker's Mac with what it made of a handed
//! question. A token reaches that one question's tools and nothing else,
//! and goes when the question is over ([`Open`] is dropped).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::oneshot;

use crate::handed::Answered;

/// One question, with what the app keeps for it (`what`): its tools, and
/// whatever they gather.
pub struct Asked<T> {
    pub what: T,
    /// Where the asker's Mac says what it made of a handed question.
    handed: Mutex<Option<oneshot::Sender<Answered>>>,
}

/// The questions being answered, by their tokens.
pub struct Questions<T>(Arc<Mutex<HashMap<String, Arc<Asked<T>>>>>);

impl<T> Default for Questions<T> {
    fn default() -> Self {
        Questions(Arc::default())
    }
}

impl<T> Clone for Questions<T> {
    fn clone(&self) -> Self {
        Questions(self.0.clone())
    }
}

impl<T> Questions<T> {
    /// Opens a token for a question, which lasts as long as the returned
    /// [`Open`].
    pub fn open(&self, what: T) -> Open<T> {
        self.opened(Asked {
            what,
            handed: Mutex::new(None),
        })
    }

    /// Opens a token for a question handed to the asker's Mac, and where
    /// to hear what it made of it.
    pub fn hand(&self, what: T) -> (Open<T>, oneshot::Receiver<Answered>) {
        let (said, heard) = oneshot::channel();
        let open = self.opened(Asked {
            what,
            handed: Mutex::new(Some(said)),
        });
        (open, heard)
    }

    fn opened(&self, asked: Asked<T>) -> Open<T> {
        let token = new_token();
        let asked = Arc::new(asked);
        if let Ok(mut open) = self.0.lock() {
            open.insert(token.clone(), asked.clone());
        }
        Open {
            token,
            asked,
            questions: self.clone(),
        }
    }

    /// The question `token` names, while it's open.
    pub fn get(&self, token: &str) -> Option<Arc<Asked<T>>> {
        self.0.lock().ok()?.get(token).cloned()
    }

    /// Hands what the asker's Mac made of a question to whoever waits for
    /// it. False when `token` names no handed question waiting: it's over,
    /// or was never handed. Once.
    pub fn answered(&self, token: &str, answered: Answered) -> bool {
        let Some(said) = self
            .get(token)
            .and_then(|asked| asked.handed.lock().ok()?.take())
        else {
            return false;
        };
        let _ = said.send(answered);
        true
    }
}

/// A question's token, closed when this is dropped.
pub struct Open<T> {
    pub token: String,
    asked: Arc<Asked<T>>,
    questions: Questions<T>,
}

impl<T> Open<T> {
    /// What the app keeps for the question.
    pub fn what(&self) -> &T {
        &self.asked.what
    }
}

impl<T> Drop for Open<T> {
    fn drop(&mut self) {
        if let Ok(mut open) = self.questions.0.lock() {
            open.remove(&self.token);
        }
    }
}

/// The token in an `Authorization: Bearer …` header.
pub fn bearer(authorization: Option<&str>) -> Option<&str> {
    authorization?
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|token| !token.is_empty())
}

fn new_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the OS random number generator works");
    hex::encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_lasts_as_long_as_its_question() {
        let questions = Questions::default();
        let open = questions.open("triage");
        let token = open.token.clone();
        assert_eq!(token.len(), 64);
        assert_eq!(questions.get(&token).map(|a| a.what), Some("triage"));
        drop(open);
        assert!(questions.get(&token).is_none());
    }

    #[tokio::test]
    async fn the_mac_says_once_what_it_made_of_a_handed_question() {
        let questions = Questions::default();
        let (open, heard) = questions.hand(());
        let answered = Answered {
            text: Some("Done.".into()),
            ..Answered::default()
        };
        assert!(questions.answered(&open.token, answered.clone()));
        assert!(!questions.answered(&open.token, answered));
        assert_eq!(heard.await.unwrap().text.as_deref(), Some("Done."));
    }

    #[test]
    fn a_question_not_handed_hears_nothing() {
        let questions = Questions::default();
        let open = questions.open(());
        assert!(!questions.answered(&open.token, Answered::default()));
        assert!(!questions.answered("nobody", Answered::default()));
    }

    #[test]
    fn the_bearer_is_read_from_the_header() {
        assert_eq!(bearer(Some("Bearer abc")), Some("abc"));
        assert_eq!(bearer(Some("Basic abc")), None);
        assert_eq!(bearer(Some("Bearer ")), None);
        assert_eq!(bearer(None), None);
    }
}
