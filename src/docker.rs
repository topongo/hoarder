use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::ShellTask;

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub(crate) enum PathFilter {
    Include(Vec<String>),
    Exclude(Vec<String>),
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "docker_type")]
pub(crate) enum DockerInputType {
    ComposeNamedVolume {
        name: String,
        #[serde(flatten)]
        filter: Option<PathFilter>,
    },
    ComposeBoundVolume {
        service: String,
        path: PathBuf,
        #[serde(flatten)]
        filter: Option<PathFilter>,
    },
    ExecStdout {
        service: String,
        task: ShellTask,
        ext: String,
    }
}
