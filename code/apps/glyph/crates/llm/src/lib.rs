//! Translating with local models, ported from Glyph's Go server.
//!
//! A [`Client`] talks to one model server (llama-server or anything with the
//! same API). It builds the prompt with the glossary terms that appear in
//! the text, reads the model's reply however it came, and retries while the
//! text drops a `{{ }}` placeholder or a glossary term. [`Models`] is the
//! operator's list of models; [`Processes`] starts and stops the ones the
//! server runs itself.

mod check;
mod client;
pub mod languages;
mod models;
mod parse;
mod process;
mod prompt;

pub use check::{GlossaryTerm, Violation, glossary_violations, placeholders_kept};
pub use client::{BROKEN_CONFIDENCE, Client, LlmError, RETRIES, Request, Translated};
pub use models::{Model, Models, ModelsError};
pub use process::{ProcessError, Processes, Status, find_program};
