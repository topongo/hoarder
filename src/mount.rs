use std::path::PathBuf;

use crate::{docker::PathExclude, DockerBinding};

pub(crate) struct MountEntry {
    volume: String,
    mount_point: PathBuf,
    filter: Option<PathExclude>,
}

impl std::fmt::Debug for MountEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MountEntry {{ {}: {:?} ({:?}) }}", self.volume, self.mount_point, self.filter)
    }
}

impl MountEntry {
    pub(crate) fn new(volume: String, mount_point: PathBuf, filter: Option<PathExclude>) -> Self {
        Self {
            volume,
            mount_point,
            filter,
        }
    }

    pub(crate) fn build(self) -> (DockerBinding, Option<PathExclude>) {
        let Self { volume, mount_point, filter } = self;
        (
            DockerBinding { volume, path: mount_point.clone() }, 
            filter,
        )
    }
}
