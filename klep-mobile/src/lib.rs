// Copyright: klepci contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! UniFFI bindings of [`klep_core::Core`] for Android (Kotlin) and iOS
//! (Swift). Thin records + one object; all logic stays in `klep-core`.

use std::sync::Arc;
use std::sync::Mutex;

#[derive(uniffi::Record)]
pub struct DeckInfo {
    pub id: i64,
    pub name: String,
}

#[derive(uniffi::Record)]
pub struct CardView {
    pub id: i64,
    pub question: String,
}

#[derive(uniffi::Record)]
pub struct AnswerView {
    pub id: i64,
    pub question: String,
    pub answer: String,
}

#[derive(Debug, uniffi::Error)]
pub enum MobileError {
    Failed(String),
}

impl std::fmt::Display for MobileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MobileError::Failed(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for MobileError {}

impl From<klep_core::CoreError> for MobileError {
    fn from(err: klep_core::CoreError) -> Self {
        MobileError::Failed(err.to_string())
    }
}

#[derive(uniffi::Object)]
pub struct MobileCore {
    inner: Mutex<klep_core::Core>,
}

#[uniffi::export]
impl MobileCore {
    #[uniffi::constructor]
    pub fn open(path: String) -> Result<Arc<Self>, MobileError> {
        Ok(Arc::new(MobileCore {
            inner: Mutex::new(
                klep_core::Core::open(std::path::Path::new(&path)).map_err(MobileError::from)?,
            ),
        }))
    }

    pub fn decks(&self) -> Result<Vec<DeckInfo>, MobileError> {
        let core = self
            .inner
            .lock()
            .map_err(|err| MobileError::Failed(err.to_string()))?;
        Ok(core
            .decks()
            .map_err(MobileError::from)?
            .into_iter()
            .map(|deck| DeckInfo {
                id: deck.id,
                name: deck.name,
            })
            .collect())
    }

    pub fn next_card(&self) -> Result<Option<CardView>, MobileError> {
        let mut core = self
            .inner
            .lock()
            .map_err(|err| MobileError::Failed(err.to_string()))?;
        Ok(core
            .next_card()
            .map_err(MobileError::from)?
            .map(|card| CardView {
                id: card.id,
                question: card.question,
            }))
    }

    pub fn show_answer(&self, card_id: i64) -> Result<AnswerView, MobileError> {
        let mut core = self
            .inner
            .lock()
            .map_err(|err| MobileError::Failed(err.to_string()))?;
        core.show_answer(card_id)
            .map_err(MobileError::from)
            .map(|answer| AnswerView {
                id: answer.id,
                question: answer.question,
                answer: answer.answer,
            })
    }

    pub fn answer(&self, card_id: i64, rating: u8, secs: u32) -> Result<(), MobileError> {
        let mut core = self
            .inner
            .lock()
            .map_err(|err| MobileError::Failed(err.to_string()))?;
        core.answer(card_id, rating, secs)
            .map_err(MobileError::from)
    }

    pub fn timing_json(&self, search: String, top_n: u32) -> Result<String, MobileError> {
        let mut core = self
            .inner
            .lock()
            .map_err(|err| MobileError::Failed(err.to_string()))?;
        core.timing_json(&search, top_n).map_err(MobileError::from)
    }
}

uniffi::setup_scaffolding!();
