use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{either::Either, ShellTask};

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub(crate) struct PathExclude(pub(crate) Vec<PathBuf>);

impl PathExclude {
    pub(crate) fn join(self, x: impl AsRef<Path>) -> Self {
        Self(self.0.into_iter()
            .map(|p| p.join(x.as_ref()))
            .collect())
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "docker_type")]
pub(crate) enum DockerInputType {
    ComposeNamedVolume {
        name: String,
        #[serde(flatten)]
        filter: Option<PathExclude>,
    },
    ComposeBoundVolume {
        service: String,
        path: PathBuf,
        #[serde(flatten)]
        filter: Option<PathExclude>,
    },
    ExecStdout {
        service: String,
        task: ShellTask,
        ext: String,
    }
}

pub(crate) enum DockerSubcommand {
    Compose {
        project: Either<String, PathBuf>,
        subcommand: DockerComposeSubcommand,
        options: Vec<String>,
        options_inner: Vec<String>,
    },
    Volume {
        subcommand: DockerVolumeSubcommand
    },
    Container {
        subcommand: DockerContainerSubcommand,
        options: Vec<String>,
    },
    Run {
        image: String,
        volumes: Vec<DockerBinding>,
        options: Vec<String>,
        options_inner: Vec<String>,
    },
    Exec {
        service: String,
        task: ShellTask,
        options: Vec<String>,
    },
    Stop {
        service: String,
        options: Vec<String>,
    },
}

impl DockerSubcommand {
    pub(crate) fn compose(
        project: Either<String, PathBuf>,
        subcommand: DockerComposeSubcommand,
        options: Vec<impl ToString>,
        options_inner: Vec<impl ToString>,
    ) -> Self {
        Self::Compose {
            project,
            subcommand,
            options: options.into_iter().map(|s| s.to_string()).collect(),
            options_inner: options_inner.into_iter().map(|s| s.to_string()).collect(),
        }
    }

    pub(crate) fn volume(subcommand: DockerVolumeSubcommand) -> Self {
        Self::Volume { subcommand }
    }

    pub(crate) fn container(subcommand: DockerContainerSubcommand, options: Vec<impl ToString>) -> Self {
        Self::Container {
            subcommand,
            options: options.into_iter().map(|s| s.to_string()).collect(),
        }
    }

    pub(crate) fn run(
        image: impl ToString,
        volumes: Vec<DockerBinding>,
        options: Vec<impl ToString>,
        options_inner: Vec<impl ToString>,
    ) -> Self {
        Self::Run {
            image: image.to_string(),
            volumes,
            options: options.into_iter().map(|s| s.to_string()).collect(),
            options_inner: options_inner.into_iter().map(|s| s.to_string()).collect(),
        }
    }

    pub(crate) fn exec(
        service: impl ToString,
        task: ShellTask,
        options: Vec<impl ToString>,
    ) -> Self {
        Self::Exec {
            service: service.to_string(),
            task,
            options: options.into_iter().map(|s| s.to_string()).collect(),
        }
    }

    pub(crate) fn stop(
        service: impl ToString,
        options: Vec<impl ToString>,
    ) -> Self {
        Self::Stop {
            service: service.to_string(),
            options: options.into_iter().map(|s| s.to_string()).collect(),
        }
    }
}

pub(crate) enum DockerComposeSubcommand {
    Exec {
        service: String,
        task: ShellTask,
    },
    Run {
        service: String,
        task: ShellTask,
    },
    Ps(Vec<String>),
}

pub(crate) enum DockerVolumeSubcommand {
    Inspect {
        volume: String,
    }
}

impl DockerVolumeSubcommand {
    pub(crate) fn inspect(volume: impl ToString) -> Self {
        Self::Inspect { volume: volume.to_string() }
    }
}

pub(crate) enum DockerContainerSubcommand {
    Inspect {
        container: String,
    },
}

pub(crate) struct DockerCommand {
    pub(crate) subcommand: DockerSubcommand,
    pub(crate) context: Option<String>,
}

impl DockerCommand {
    pub(crate) fn new(subcommand: DockerSubcommand, context: Option<String>) -> Self {
        Self { subcommand, context }
    }

    pub(crate) fn into_command(self) -> std::process::Command {
        let mut command = std::process::Command::new("docker");
        if let Some(context) = self.context {
            command.arg("-c").arg(context);
        }

        match self.subcommand {
            DockerSubcommand::Compose {
                project,
                subcommand,
                options,
                options_inner,
            } => {
                command.arg("compose");
                match project {
                    Either::Left(project) => command.arg("-p").arg(project),
                    Either::Right(path) => command.arg("-f").arg(path),
                };
                command.args(options);
                match subcommand {
                    DockerComposeSubcommand::Exec { service, task } => {
                        command
                            .arg("exec")
                            .args(options_inner)
                            .arg(service)
                            .args(task.get_args());
                    }
                    DockerComposeSubcommand::Run { service, task } => {
                        command
                            .arg("run")
                            .args(options_inner)
                            .arg(service).args(task.get_args());
                    }
                    DockerComposeSubcommand::Ps(services) => {
                        command
                            .arg("ps")
                            .args(services)
                            .args(options_inner);
                    }
                };
            }
            DockerSubcommand::Volume { subcommand } => {
                command.arg("volume");
                match subcommand {
                    DockerVolumeSubcommand::Inspect { volume } => {
                        command.arg("inspect").arg(volume);
                    }
                };
            }
            DockerSubcommand::Container { subcommand, options } => {
                command.arg("container");
                match subcommand {
                    DockerContainerSubcommand::Inspect { container } => {
                        command.arg("inspect").arg(container);
                    }
                };
                command.args(options);
            }
            DockerSubcommand::Run {
                image,
                volumes,
                options,
                options_inner,
            } => {
                command.arg("run");
                for binding in volumes {
                    command.arg("-v").arg(binding.into_arg());
                }
                command.args(options);
                command.arg(image);
                command.args(options_inner);
            }
            DockerSubcommand::Exec { service, task, options } => {
                command.arg("exec");
                command.args(options);
                command.arg(service);
                command.args(task.get_args());
            }
            DockerSubcommand::Stop { service, options } => {
                command.arg("stop");
                command.arg(service);
                command.args(options);
            }
        }

        command
    }

    pub(crate) fn spawn(self) -> std::io::Result<std::process::Child> {
        self.into_command().spawn()
    }

    pub(crate) fn spawn_and_wait(self) -> std::io::Result<std::process::ExitStatus> {
        self.into_command().spawn()?.wait()
    }

    pub(crate) fn spawn_and_expect(self) {
        match self.spawn_and_wait() {
            Ok(status) if status.success() => {},
            Ok(status) => panic!("Docker command failed with status: {}", status),
            Err(e) => panic!("Failed to spawn Docker command: {}", e),
        }
    }
}

#[derive(Debug)]
pub(crate) struct DockerBinding {
    pub(crate) volume: String,
    pub(crate) path: PathBuf,
    pub(crate) flags: Option<String>,
}

impl DockerBinding {
    pub(crate) fn new_ro(volume: String, path: PathBuf) -> Self {
        Self { volume, path, flags: Some("ro".to_string()) }
    }

    pub(crate) fn new_rw(volume: String, path: PathBuf) -> Self {
        Self { volume, path, flags: None }
    }

    pub(crate) fn into_arg(self) -> String {
        format!("{}:{}{}", self.volume, self.path.display(), self.flags.map_or("".to_owned(), |f| format!(":{}", f)))
    }
}
