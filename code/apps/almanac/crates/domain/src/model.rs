use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

pub use code_villain_sync::{Actor, GuestView, ValidationError, Write};

use crate::talk::{MessageCreate, MessagePatch, TalkCreate, TalkPatch};
use crate::topic::{
    BlockCreate, BlockPatch, PointCreate, PointPatch, QuestionCreate, QuestionPatch,
    ReminderCreate, ReminderPatch, TopicCreate, TopicPatch,
};

/// Every kind of record that clients sync: a workspace's topics, what each
/// has kept and the talks it was kept from, and the workspace's members.
///
/// To add one: add a variant, a migration for its table, and its record,
/// create and patch types; the compiler points at every `match` to extend.
/// See "Adding a synced model" in AGENTS.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum ModelKind {
    Topic,
    Block,
    Talk,
    Message,
    Point,
    Reminder,
    Question,
    User,
}

impl ModelKind {
    pub const ALL: &[ModelKind] = &[
        ModelKind::Topic,
        ModelKind::Block,
        ModelKind::Talk,
        ModelKind::Message,
        ModelKind::Point,
        ModelKind::Reminder,
        ModelKind::Question,
        ModelKind::User,
    ];

    /// The table (or view) holding this kind of record.
    pub fn table(self) -> &'static str {
        match self {
            ModelKind::Topic => "topics",
            ModelKind::Block => "blocks",
            ModelKind::Talk => "talks",
            ModelKind::Message => "messages",
            ModelKind::Point => "points",
            ModelKind::Reminder => "reminders",
            ModelKind::Question => "questions",
            ModelKind::User => "workspace_users",
        }
    }

    /// The name used on the wire and in `sync_log.model`.
    pub fn name(self) -> &'static str {
        match self {
            ModelKind::Topic => "Topic",
            ModelKind::Block => "Block",
            ModelKind::Talk => "Talk",
            ModelKind::Message => "Message",
            ModelKind::Point => "Point",
            ModelKind::Reminder => "Reminder",
            ModelKind::Question => "Question",
            ModelKind::User => "User",
        }
    }

    pub fn from_name(name: &str) -> Option<ModelKind> {
        ModelKind::ALL.iter().copied().find(|m| m.name() == name)
    }

    /// Whether clients may make this kind of write through sync. Topics
    /// start and talks are had through the API, because Almanac answers
    /// them; what a talk said is the server's. Users are read-only:
    /// membership changes go through the account API.
    pub fn client_may(self, write: Write) -> bool {
        match self {
            ModelKind::Talk => write == Write::Delete,
            ModelKind::Message | ModelKind::User => false,
            ModelKind::Topic | ModelKind::Block | ModelKind::Point | ModelKind::Reminder => true,
            ModelKind::Question => write != Write::Create,
        }
    }

    /// Records only their owner (`user_id`) syncs, rather than the whole
    /// workspace. Almanac has none yet; the engine supports them as Forge's does.
    pub fn private_to_user(self) -> bool {
        false
    }

    /// What a guest syncs: no topics yet, and the people who aren't guests.
    pub fn guest_view(self) -> GuestView {
        match self {
            ModelKind::User => GuestView::Where("t.id = $viewer OR t.role <> 'guest'"),
            _ => GuestView::Hidden,
        }
    }

    /// Checks the fields sent to create a record.
    pub fn validate_create(self, data: &Map<String, Value>) -> Result<(), ValidationError> {
        match self {
            ModelKind::Topic => check::<TopicCreate>(data),
            ModelKind::Block => check::<BlockCreate>(data),
            ModelKind::Talk => check::<TalkCreate>(data),
            ModelKind::Message => check::<MessageCreate>(data),
            ModelKind::Point => check::<PointCreate>(data),
            ModelKind::Reminder => check::<ReminderCreate>(data),
            ModelKind::Question => check::<QuestionCreate>(data),
            ModelKind::User => Err(ValidationError("users are read-only".into())),
        }
    }

    /// Checks the fields sent to update a record.
    pub fn validate_patch(self, data: &Map<String, Value>) -> Result<(), ValidationError> {
        match self {
            ModelKind::Topic => check::<TopicPatch>(data),
            ModelKind::Block => check::<BlockPatch>(data),
            ModelKind::Talk => check::<TalkPatch>(data),
            ModelKind::Message => check::<MessagePatch>(data),
            ModelKind::Point => check::<PointPatch>(data),
            ModelKind::Reminder => check::<ReminderPatch>(data),
            ModelKind::Question => check::<QuestionPatch>(data),
            ModelKind::User => Err(ValidationError("users are read-only".into())),
        }
    }
}

/// How the sync engine knows Almanac's models.
impl code_villain_sync::Model for ModelKind {
    const ALL: &'static [ModelKind] = ModelKind::ALL;

    fn table(self) -> &'static str {
        ModelKind::table(self)
    }

    fn name(self) -> &'static str {
        ModelKind::name(self)
    }

    fn client_may(self, write: Write) -> bool {
        ModelKind::client_may(self, write)
    }

    fn private_to_user(self) -> bool {
        ModelKind::private_to_user(self)
    }

    fn validate_create(self, data: &Map<String, Value>) -> Result<(), ValidationError> {
        ModelKind::validate_create(self, data)
    }

    fn validate_patch(self, data: &Map<String, Value>) -> Result<(), ValidationError> {
        ModelKind::validate_patch(self, data)
    }

    fn guest_view(self) -> GuestView {
        ModelKind::guest_view(self)
    }
}

/// Business rules a create or patch input must satisfy, beyond its types.
pub trait Validate {
    fn validate(&self) -> Result<(), ValidationError>;
}

/// Parses `data` as `T`, which rejects unknown fields and wrong types, then
/// applies `T`'s rules.
fn check<T: DeserializeOwned + Validate>(data: &Map<String, Value>) -> Result<(), ValidationError> {
    let input: T = serde_json::from_value(Value::Object(data.clone()))
        .map_err(|e| ValidationError(e.to_string()))?;
    input.validate()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn map(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn names_round_trip() {
        for model in ModelKind::ALL {
            assert_eq!(ModelKind::from_name(model.name()), Some(*model));
        }
    }

    #[test]
    fn what_a_talk_said_is_the_servers() {
        for write in [Write::Create, Write::Update, Write::Delete] {
            assert!(!ModelKind::Message.client_may(write));
        }
        assert!(ModelKind::Topic.client_may(Write::Create));
        assert!(ModelKind::Topic.client_may(Write::Update));
        let point = map(
            json!({ "topic_id": uuid::Uuid::nil(), "block_id": uuid::Uuid::nil(), "theme": "Monstera", "text": "Erst gießen, wenn die Erde trocken ist." }),
        );
        assert!(ModelKind::Point.validate_create(&point).is_ok());
    }

    #[test]
    fn users_are_read_only() {
        for write in [Write::Create, Write::Update, Write::Delete] {
            assert!(!ModelKind::User.client_may(write));
        }
    }
}
