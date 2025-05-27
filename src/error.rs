use std::fmt::Display;

use serde::Serialize;

#[derive(Serialize, Debug)]
pub(crate) struct SerializableError {
    message: String,
}

impl SerializableError {
    pub fn new(message: impl ToString) -> Self {
        SerializableError { message: message.to_string() }
    }

    #[allow(dead_code)]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl Display for SerializableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Error {{ {} }}", self.message)
    }
}

impl std::error::Error for SerializableError {}

impl From<std::io::Error> for SerializableError {
    fn from(e: std::io::Error) -> Self {
        SerializableError {
            message: e.to_string(),
        }
    }
}

impl From<serde_json::Error> for SerializableError {
    fn from(e: serde_json::Error) -> Self {
        SerializableError {
            message: e.to_string(),
        }
    }
}
