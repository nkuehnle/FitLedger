pub mod balance;
pub mod body;
pub mod cardio;
pub mod db;
pub mod exercises;
pub mod export;
pub mod health;
pub mod history;
pub mod library;
pub mod live;
pub mod matcher;
pub mod norms;
pub mod planning;
pub mod report;
pub mod review;
pub mod routines;
pub mod stats;
pub mod strong;
pub mod targets;
pub mod templates;
pub mod units;
pub mod wearables;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("{0}")]
    Format(String),
    #[error("the file doesn't say whether weights are in kg or lb")]
    MissingWeightUnit,
}

pub type Result<T> = std::result::Result<T, Error>;

/// Kind of set. Strong marks these with letters in the `Set` column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SetType {
    Normal,
    Warmup,
    Drop,
    Failure,
}

impl SetType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Warmup => "warmup",
            Self::Drop => "drop",
            Self::Failure => "failure",
        }
    }
}
