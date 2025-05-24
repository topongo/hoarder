use std::path::PathBuf;

use crate::{docker::PathExclude, ShellTask};

#[derive(Debug)]
pub(crate) struct ResticBackup {
    path: PathBuf,
    /// exclude string globs
    excludes: Vec<String>
}

impl ResticBackup {
    pub(crate) fn with_excludes(path: PathBuf, excludes: Vec<PathExclude>) -> Self {
        Self {
            excludes: excludes.into_iter()
                .flat_map(|pe| pe.0)
                .map(|p| p.join(&path).to_string_lossy().to_string())
                .collect(),
            path,
        }
    }

    pub(crate) fn new(path: PathBuf) -> Self {
        Self {
            excludes: vec![],
            path,
        }
    }

    pub(crate) fn into_task(self) -> ShellTask {
        let mut task = ShellTask::new("restic");
        task
            .arg("backup")
            .arg(self.path.to_string_lossy().to_string())
            .args(["--tag", "hoarder"]);
        for exclude in self.excludes {
            task.arg("--exclude");
            task.arg(exclude);
        }
        task
    }
}
