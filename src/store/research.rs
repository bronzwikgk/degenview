//! Local persistence for Research Runner records.

use crate::store::Store;
use serde::Serialize;
use serde::de::DeserializeOwned;

const PREFIX: &str = "research.v1.";

impl Store {
    pub fn research_get<T: DeserializeOwned>(&self, name: &str) -> Option<T> {
        self.setting(&format!("{PREFIX}{name}"))
    }

    pub fn research_set<T: Serialize>(&self, name: &str, value: &T) -> rusqlite::Result<()> {
        self.set_setting(&format!("{PREFIX}{name}"), value)
    }
}
