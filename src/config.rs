use serde::{Deserialize, Serialize};

use crate::service::Service;

static BASE_PATH: &str = "./output";
static RESTIC_BASE_PATH: &str = "/backup";
static RESTIC_IMAGE: &str = "test";

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct FullConfig {
    pub(crate) services: Vec<Service>,
    #[serde(flatten)]
    pub(crate) config: Config,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Config {
    /// where temporary data will be stored/mounted inside the restic container
    base_path: Option<String>,
    /// the restic image to use
    restic_image: Option<String>,
    /// the restic path to back up once inside the container
    resitc_base_path: Option<String>,
    /// whether to run in dry run mode
    #[serde(default)]
    pub(crate) dry_run: bool,
}

impl Config {
    fn _get_env(&self, name: &str) -> Option<String> {
        match std::env::var(format!("HOARDER_{}", name)) {
            Ok(val) => if val.is_empty() {
                None
            } else {
                Some(val)
            },
            Err(std::env::VarError::NotPresent) => None,
            Err(e) => panic!("{:?}", e),
        }
    }

    pub fn base_path(&self) -> String {
        self._get_env("BASE_PATH")
            .or_else(|| self.base_path.clone())
            .unwrap_or(BASE_PATH.to_string())
    }

    pub fn restic_image(&self) -> String {
        self._get_env("RESTIC_IMAGE")
            .or_else(|| self.restic_image.clone())
            .unwrap_or(RESTIC_IMAGE.to_string())
    }

    pub fn restic_base_path(&self) -> String {
        self._get_env("RESTIC_BASE_PATH")
            .or_else(|| self.resitc_base_path.clone())
            .unwrap_or(RESTIC_BASE_PATH.to_string())
    }
}
