//! Questions handed to the asker's Mac. Claude Code on your Mac answers
//! first, wherever the workspace is (`studio/design/behaviour.md`,
//! "Claude"): on a server's workspace the desktop app asks with `here`,
//! and the server writes the question as it would for its own Claude Code,
//! opens its token and hands it over ([`HandedQuestion`]). Claude Code
//! there calls Almanac's tools at this server's `/api/mcp` with the token,
//! and the app says what it made of it at `/api/answered` ([`Answered`]).
//! What the tools kept is written here, with the answer, as any other.
//! The Mac's half is [`answer`], which the desktop app runs.

use std::future::Future;
use std::time::Duration;

use almanac_domain::{Answered, HandedPhoto, HandedQuestion, NotAnswered};
use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};
use tokio::sync::oneshot;

use crate::acp::{Adapter, Answer, Failure, Question};
use crate::photos::Image;
use crate::state::AppState;
use crate::topics::cut;

/// Starting Claude Code on the Mac, and its answer, searching included.
const HANDED_FOR: Duration = Duration::from_secs(11 * 60);
const STOPPED: &str = "Claude Code on your Mac stopped before it answered. Ask again.";

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
        first_photo: u32::try_from(question.first_photo).ok(),
        web: question.web,
        tools: question.tools.iter().map(|t| t.to_string()).collect(),
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
            id: uuid::Uuid::nil(),
            media_type: photo.media_type,
            data: photo.data,
        })
        .collect();
    let tools: Vec<&str> = question.tools.iter().map(String::as_str).collect();
    let asked = Question {
        system: &question.system,
        prompt: &question.prompt,
        images: &images,
        first_photo: question.first_photo.map_or(1, |n| n.max(1) as usize),
        web: question.web,
        tools: &tools,
        token: &question.token,
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
/// once it says, unless the asker is `gone` first or it takes too long.
pub async fn wait(
    answered: oneshot::Receiver<Answered>,
    gone: impl Future<Output = ()>,
) -> Result<Answer, Failure> {
    let stopped = || Failure::Trouble(STOPPED.into());
    let answered = tokio::select! {
        answered = answered => answered.map_err(|_| stopped())?,
        () = gone => return Err(stopped()),
        () = tokio::time::sleep(HANDED_FOR) => {
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
                .map(|said| cut(&said, 300))
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

/// `POST /api/answered`: what the Mac made of the question its bearer
/// names. Once: the question is over after.
pub async fn answered(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(answered): Json<Answered>,
) -> StatusCode {
    let said = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .and_then(|token| state.questions.get(token))
        .and_then(|asking| asking.handed.lock().ok()?.take());
    match said {
        Some(said) => {
            let _ = said.send(answered);
            StatusCode::NO_CONTENT
        }
        None => StatusCode::UNAUTHORIZED,
    }
}
