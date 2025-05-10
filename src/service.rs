use serde::{Deserialize, Serialize};

use crate::archive::ArchiveOptions;

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Service {
    pub(crate) name: String,
    pub(crate) archives: Vec<ArchiveOptions>,
    pub(crate) compose_project: Option<String>,
}
