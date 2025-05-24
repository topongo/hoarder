use serde::{Deserialize, Serialize};

use crate::{service::Service, DockerCommand, DockerSubcommand};

static RESTIC_ROOT: &str = "/restic";
static RESTIC_IMAGE: &str = "test";
static RESTIC_CONTAINER_NAME: &str = "hoarder-restic";

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct FullConfig {
    pub(crate) services: Vec<Service>,
    pub(crate) hooks: HookConfig,
    #[serde(flatten)]
    pub(crate) config: Config,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Config {
    /// where temporary data will be stored/mounted inside the restic container
    restic_root: Option<String>,
    /// the restic image to use
    restic_image: Option<String>,
    /// the restic path to back up once inside the container
    intermediate_path: Option<String>,
    /// directory to mount in restic container.
    /// used if using a docker in docker setup: if the intermediate_path is /data in the host and
    /// /int in the first container, this should be set to /data, as the second container will
    /// always mount from the host and not the first container
    intermediate_mount_override: Option<String>,
    /// the restic password file to use
    restic_password_file: Option<String>,
    /// restic host to use
    restic_host: Option<String>,
    /// the restic container name/id to use
    restic_container_name: Option<String>,
    /// whether to run in dry run mode
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    pub(crate) docker_context: Option<String>,
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

    pub fn restic_root(&self) -> String {
        self._get_env("RESTIC_ROOT")
            .or_else(|| self.restic_root.clone())
            .unwrap_or(RESTIC_ROOT.to_string())
    }

    pub fn restic_image(&self) -> String {
        self._get_env("RESTIC_IMAGE")
            .or_else(|| self.restic_image.clone())
            .unwrap_or(RESTIC_IMAGE.to_string())
    }

    pub fn restic_password_file(&self) -> String {
        self._get_env("RESTIC_PASSWORD_FILE")
            .expect("restic_password_file must be set")
    }

    pub fn restic_host(&self) -> String {
        self._get_env("RESTIC_HOST")
            .or_else(|| self.restic_host.clone())
            .expect("restic_host must be set")
    }

    pub fn restic_container_name(&self) -> String {
        self._get_env("RESTIC_CONTAINER_NAME")
            .or_else(|| self.restic_container_name.clone())
            .unwrap_or(RESTIC_CONTAINER_NAME.to_string())
    }

    pub fn intermediate_path(&self) -> String {
        self._get_env("INTERMEDIATE")
            .or_else(|| self.intermediate_path.clone())
            .expect("intermediate_path must be set")
    }

    pub fn intermediate_mount_override(&self) -> Option<String> {
        self._get_env("INTERMEDIATE_MOUNT_OVERRIDE")
            .or_else(|| self.intermediate_mount_override.clone())
    }

    pub fn docker_command_with_context(&self, subcommand: DockerSubcommand) -> DockerCommand {
        DockerCommand::new(
            subcommand,
            self.docker_context.clone(),
        )
    }

    pub fn dry_run(&self) -> bool {
        self._get_env("DRY_RUN")
            .or_else(|| Some(self.dry_run.to_string()))
            .unwrap_or("false".to_string())
            .parse()
            .unwrap()
    }
}
