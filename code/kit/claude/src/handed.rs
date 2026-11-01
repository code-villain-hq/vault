//! Questions handed to the asker's Mac. Claude Code on your Mac answers
//! first, wherever the workspace is (`studio/design/behaviour.md`,
//! "Claude"): on a server's workspace the desktop app asks with `here`,
//! and the server writes the question as it would for its own Claude Code,
//! opens its token ([`crate::questions`]) and hands it over
//! ([`HandedQuestion`], from [`question`]). Claude Code there calls the
//! app's tools at the server's MCP endpoint with the token, and the app
//! says what it made of it at `/api/answered` ([`Answered`]), which the
//! server waits for ([`wait`]). The Mac's half is [`answer`].

use std::future::Future;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::acp::{ANSWER_TIMEOUT, Adapter, Answer, Failure, Image, Question};

/// Starting Claude Code on the Mac, on top of the answer's own time.
const STARTING: Duration = Duration::from_secs(60);
const STOPPED: &str = "Claude Code on your Mac stopped before it answered. Ask again.";

/// A question a server hands to the asker's Mac, for Claude Code there.
/// It reaches the app's tools at the server's MCP endpoint with `token`,
/// which lasts as long as the question, and says what it made of it at
/// `/api/answered` with the same token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandedQuestion {
    pub token: String,
    /// The app's prompt: how to answer, and what about.
    pub system: String,
    /// What they ask.
    pub prompt: String,
    /// Pictures, after the prompt.
    pub photos: Vec<HandedPhoto>,
    /// Whether it may search the web and read pages.
    pub web: bool,
    /// The app's tools it may use.
    pub tools: Vec<String>,
    /// How long the answer may take, in seconds; ten minutes when not said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    /// How many turns it may take; as many as it likes when not said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandedPhoto {
    pub media_type: String,
    pub data: String,
}

/// Why the Mac gave no answer. Claude Code not installed or signed out
/// hands the question back, and the server's key answers when it has one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotAnswered {
    NotInstalled,
    SignedOut,
    Trouble,
}

/// What Claude Code on the asker's Mac made of a [`HandedQuestion`]
/// (`POST /api/answered`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Answered {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// The pages it read, in order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read: Option<Vec<String>>,
    /// Why it stopped: `end_turn`, `max_tokens`, `refusal`, `cancelled`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_answered: Option<NotAnswered>,
    /// What went wrong, in its words, with `trouble`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub said: Option<String>,
}

/// What the Mac needs to answer `question`.
pub fn question(question: &Question<'_>) -> HandedQuestion {
    HandedQuestion {
        token: question.token.to_string(),
        system: question.system.to_string(),
        prompt: question.prompt.to_string(),
        photos: question
            .images
            .iter()
            .map(|image| HandedPhoto {
                media_type: image.media_type.clone(),
                data: image.data.clone(),
            })
            .collect(),
        web: question.web,
        tools: question.tools.iter().map(|t| t.to_string()).collect(),
        timeout: question.timeout.map(|t| t.as_secs()),
        max_turns: question.max_turns,
    }
}

/// The Mac's half: Claude Code there answers a handed question with
/// `adapter`, whose MCP endpoint is the server's, each piece to `piece` as
/// it's written, and says what came of it.
pub async fn answer(
    adapter: &Adapter,
    question: HandedQuestion,
    piece: impl FnMut(&str) + Send,
) -> Answered {
    let images: Vec<Image> = question
        .photos
        .into_iter()
        .map(|photo| Image {
            media_type: photo.media_type,
            data: photo.data,
        })
        .collect();
    let tools: Vec<&str> = question.tools.iter().map(String::as_str).collect();
    let asked = Question {
        system: &question.system,
        prompt: &question.prompt,
        images: &images,
        web: question.web,
        tools: &tools,
        token: &question.token,
        timeout: question.timeout.map(Duration::from_secs),
        max_turns: question.max_turns,
    };
    let not_answered = |why| Answered {
        not_answered: Some(why),
        ..Answered::default()
    };
    match adapter.ask(&asked, piece).await {
        Ok(answer) => Answered {
            text: Some(answer.text),
            read: Some(answer.read),
            stop: Some(answer.stop),
            ..Answered::default()
        },
        Err(Failure::NotInstalled) => not_answered(NotAnswered::NotInstalled),
        Err(Failure::SignedOut) => not_answered(NotAnswered::SignedOut),
        Err(Failure::Trouble(said)) => Answered {
            said: Some(said),
            ..not_answered(NotAnswered::Trouble)
        },
    }
}

/// What the Mac made of a handed question, as Claude Code's own answer:
/// once it says, unless the asker is `gone` first or it takes longer than
/// the question's `timeout` allows.
pub async fn wait(
    answered: oneshot::Receiver<Answered>,
    gone: impl Future<Output = ()>,
    timeout: Option<Duration>,
) -> Result<Answer, Failure> {
    let stopped = || Failure::Trouble(STOPPED.into());
    let answered = tokio::select! {
        answered = answered => answered.map_err(|_| stopped())?,
        () = gone => return Err(stopped()),
        () = tokio::time::sleep(timeout.unwrap_or(ANSWER_TIMEOUT) + STARTING) => {
            return Err(Failure::Trouble(
                "Claude Code on your Mac took too long to answer. Ask again.".into(),
            ));
        }
    };
    match answered.not_answered {
        Some(NotAnswered::NotInstalled) => Err(Failure::NotInstalled),
        Some(NotAnswered::SignedOut) => Err(Failure::SignedOut),
        Some(NotAnswered::Trouble) => Err(Failure::Trouble(
            answered
                .said
                .map(|said| cut(said.trim(), 300))
                .filter(|said| !said.is_empty())
                .unwrap_or_else(|| STOPPED.into()),
        )),
        None => Ok(Answer::said(
            answered.text.unwrap_or_default(),
            answered.read.unwrap_or_default(),
            answered.stop.unwrap_or_else(|| "end_turn".into()),
        )),
    }
}

/// `text` cut to `max` characters, with an ellipsis when it was longer.
fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max).collect();
    format!("{}…", cut.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handed(answered: Answered) -> oneshot::Receiver<Answered> {
        let (said, heard) = oneshot::channel();
        said.send(answered).unwrap();
        heard
    }

    #[tokio::test]
    async fn an_answer_comes_back_as_claude_codes_own() {
        let answered = Answered {
            text: Some("Triaged.".into()),
            ..Answered::default()
        };
        let answer = wait(handed(answered), std::future::pending(), None)
            .await
            .unwrap();
        assert_eq!(answer.text, "Triaged.");
        assert_eq!(answer.stop, "end_turn");
    }

    #[tokio::test]
    async fn not_installed_and_signed_out_hand_the_question_back() {
        let not = |why| Answered {
            not_answered: Some(why),
            ..Answered::default()
        };
        let pending = std::future::pending;
        assert_eq!(
            wait(handed(not(NotAnswered::NotInstalled)), pending(), None)
                .await
                .unwrap_err(),
            Failure::NotInstalled
        );
        assert_eq!(
            wait(handed(not(NotAnswered::SignedOut)), pending(), None)
                .await
                .unwrap_err(),
            Failure::SignedOut
        );
        let trouble = Answered {
            said: Some("  ".into()),
            ..not(NotAnswered::Trouble)
        };
        assert_eq!(
            wait(handed(trouble), pending(), None).await.unwrap_err(),
            Failure::Trouble(STOPPED.into())
        );
    }

    #[tokio::test]
    async fn a_mac_that_goes_quiet_stopped() {
        let (said, heard) = oneshot::channel::<Answered>();
        drop(said);
        assert_eq!(
            wait(heard, std::future::pending(), None).await.unwrap_err(),
            Failure::Trouble(STOPPED.into())
        );
        let (_said, heard) = oneshot::channel::<Answered>();
        assert_eq!(
            wait(heard, async {}, None).await.unwrap_err(),
            Failure::Trouble(STOPPED.into())
        );
    }

    #[test]
    fn the_wire_is_snake_case_and_strict() {
        let answered: Answered = serde_json::from_str(r#"{"not_answered":"signed_out"}"#).unwrap();
        assert_eq!(answered.not_answered, Some(NotAnswered::SignedOut));
        assert!(serde_json::from_str::<Answered>(r#"{"txt":"hi"}"#).is_err());
        let question = HandedQuestion {
            token: "t".into(),
            system: String::new(),
            prompt: String::new(),
            photos: Vec::new(),
            web: false,
            tools: Vec::new(),
            timeout: None,
            max_turns: None,
        };
        assert!(
            !serde_json::to_string(&question)
                .unwrap()
                .contains("timeout")
        );
    }
}
