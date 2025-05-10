use serde::{Deserialize, Serialize};

use crate::DockerInputType;

#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum ArchiveInput {
    Docker(DockerInputType),
    // Directory {
    //     path: PathBuf,
    //     prepare: Vec<ShellTask>,
    // },
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct ArchiveOptions {
    pub(crate) input: ArchiveInput,
    // output: OutputType,
    // mode: ArchiveMode,
    pub(crate) name: String,
}
