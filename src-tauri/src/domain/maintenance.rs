use serde::{Deserialize, Serialize};

use super::ValidationError;

pub const EXPLORATION_DATA_RESET_CONFIRMATION: &str = "RESET_EXPLORATION_DATA";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExplorationDataResetRequest {
    pub confirmation: String,
}

impl ExplorationDataResetRequest {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.confirmation == EXPLORATION_DATA_RESET_CONFIRMATION {
            Ok(())
        } else {
            Err(ValidationError::new(
                "confirmation",
                "must explicitly confirm exploration data reset",
            ))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplorationDataResetResult {
    pub favorites_removed: u64,
    pub search_history_removed: u64,
    pub auto_find_runs_removed: u64,
    pub auto_find_candidates_removed: u64,
    pub auto_find_exclusions_removed: u64,
}
