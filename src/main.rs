use archive::{ArchiveInput, ArchiveOptions};
use config::FullConfig;
use indicatif::HumanBytes;
use log::{debug, error, info, warn};
use restic::ResticBackup;
use service::Service;
use std::{fs::File, io::{BufReader, BufWriter, Read, Write}, path::PathBuf, process::{ExitStatus, Stdio}};
use serde::Deserialize;

mod config;
mod service;
mod archive;
mod task;
mod docker;
mod either;
mod mount;
mod restic;

use task::ShellTask;
use docker::{DockerBinding, DockerCommand, DockerComposeSubcommand, DockerContainerSubcommand, DockerInputType, DockerSubcommand, DockerVolumeSubcommand, PathExclude};
#[allow(unused_imports)]
use either::Either::{Left, Right};

struct SpinnerWriter<R: Read> {
    output: BufWriter<Box<dyn Write>>,
    input: BufReader<R>,
    bytes_written: usize,
    bar: indicatif::ProgressBar,
}

impl<R: Read> SpinnerWriter<R> {
    fn write_all(&mut self) -> std::io::Result<()> {
        let mut buffer = [0; 10 << 10];
        loop {
            let bytes_read = self.input.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            self.output.write_all(&buffer[..bytes_read])?;
            self.bytes_written += bytes_read;
            self.bar.set_position(self.bytes_written as u64);
            self.bar.set_message(format!("{}", HumanBytes(self.bytes_written as u64)));
            self.output.flush()?;
        }
        self.output.flush()?;
        Ok(())
    }
}

fn main() -> std::io::Result<()> {
    pretty_env_logger::init();

    let config = std::fs::read_to_string("config.yaml").expect("Failed to read config file");
    let FullConfig { services, config } = serde_yaml::from_str(&config).expect("Failed to parse config file");

    info!("Backup summary:");
    for service in &services {
        info!("- {}:", service.name);
        for archive in &service.archives {
            info!("  - {}: {:?}", archive.name, archive.input);
        }
    }
    info!("");

    let mut backups: Vec<ResticBackup> = vec![];
    let mut mounts: Vec<DockerBinding> = vec![
        DockerBinding::new_ro(
            config.restic_root(),
            PathBuf::from(config.intermediate_mount_override().unwrap_or(config.intermediate_path())),
        ),
        DockerBinding::new_ro(
            config.restic_password_file(),
            PathBuf::from("/restic_password"),
        )
    ];

    for service in services {
        debug!("{}: service: {:?}", service.name, service);
        let Service { archives, compose_project, name: service_name } = service;
        let compose_project = compose_project.unwrap_or(service_name.clone());
        let mut excludes = vec![];
        for archive in archives {
            debug!("{}: {}: archive: {:?}", service_name, compose_project, archive);
            let ArchiveOptions { input, name: archive_name } = archive;
            match input {
                ArchiveInput::Docker(docker_input) => match docker_input {
                    DockerInputType::ExecStdout { service, task, ext } => {
                        info!("{}: {}: using mode: ExecStdout", service_name, archive_name);

                        let dcommand = config.docker_command_with_context(
                            DockerSubcommand::Compose {
                                project: Left(compose_project.clone()),
                                subcommand: DockerComposeSubcommand::Exec {
                                    service: service.clone(),
                                    task: task.clone(),
                                },
                                options: vec![],
                                options_inner: vec!["-i".to_owned()],
                            },
                        );
                        let mut command = dcommand.into_command();
                        let output_path = PathBuf::from(config.intermediate_path()).join(&service_name);
                        std::fs::create_dir_all(&output_path)?;
                        let output_name = format!("{}.{}", archive_name, ext);
                        let output_file = output_path.join(output_name);
                        debug!("{}: {}: ExecStdout: output file: {:?}", service_name, archive_name, output_file);

                        command
                            .stderr(std::process::Stdio::piped())
                            .stdout(Stdio::piped());
                        debug!("{}: {}: ExecStdout: executing command: {:?}", service_name, archive_name, command.get_args().collect::<Vec<_>>());
                        let mut handle = command.spawn().expect("Failed to start task");
                        let stdout = handle.stdout.take().expect("Failed to get stdout");
                        let mut proxy = if config.dry_run() {
                            warn!("{}: {}: dry run mode, not writing to file {}", service_name, archive_name, output_file.display());
                            SpinnerWriter {
                                output: BufWriter::new(Box::new(std::io::sink())),
                                input: BufReader::new(stdout),
                                bytes_written: 0,
                                bar: indicatif::ProgressBar::new_spinner(),
                            }
                        } else {
                            let output = File::create(&output_file)?;
                            SpinnerWriter {
                                output: BufWriter::new(Box::new(output)),
                                input: BufReader::new(stdout),
                                bytes_written: 0,
                                bar: indicatif::ProgressBar::new_spinner(),
                            }
                        };
                        proxy.write_all().expect("Failed to write to output file");

                        let status = handle.wait().expect("Failed to wait for task");
                        if !status.success() {
                            error!("{}: {}: docker exec stdout failure: {}", service_name, archive_name, status);
                            if let Some(mut stderr) = handle.stderr {
                                let mut buf = String::new();
                                stderr.read_to_string(&mut buf).expect("Failed to read stderr");
                                if !buf.is_empty() && buf != "\n" {
                                    error!("stderr output:");
                                    for line in buf.lines() {
                                        error!("=> {}", line);
                                    }
                                    continue;
                                }
                            }
                            error!("no stderr output");
                        }
                    }
                    DockerInputType::ComposeNamedVolume { name, filter } => {
                        info!("{}: {}: using mode: ComposeNamedVolume", service_name, archive_name);
                        let global_volume_name = format!("{compose_project}_{name}");
                        debug!("{}: {}: ComposeNamedVolume: using canonical volume name: {}", service_name, archive_name, global_volume_name);
                        let output = PathBuf::from(config.restic_root()).join(&service_name).join(&archive_name);
                        // ensure global volume exists
                        let mut command = config
                            .docker_command_with_context(DockerSubcommand::volume(DockerVolumeSubcommand::inspect(&global_volume_name)))
                            .into_command();
                        command
                            .stderr(Stdio::null())
                            .stdout(Stdio::null());
                        debug!("{}: {}: ComposeNamedVolume: inspecting volume: docker {:?}", service_name, archive_name, command.get_args().collect::<Vec<_>>());
                        let status = command.status().expect("Failed to check volume");
                        if !status.success() {
                            error!("{}: {}: ComposeNamedVolume: volume {} does not exist", service_name, archive_name, global_volume_name);
                        } else {
                            mounts.push(DockerBinding::new_ro(global_volume_name, output));
                            if let Some(filter) = filter {
                                excludes.push(filter.join(&archive_name));
                            }
                        }
                    }
                    DockerInputType::ComposeBoundVolume { service, path, filter } => {
                        info!("{}: {}: using mode: ComposeBoundVolume", service_name, archive_name);
                        let output = PathBuf::from(config.restic_root()).join(&service_name).join(&archive_name);
                        // find the bound volume inside the service
                        let mut command = config.docker_command_with_context(DockerSubcommand::compose(
                            Left(compose_project.clone()),
                            DockerComposeSubcommand::Ps(vec![service]),
                            Vec::<String>::new(),
                            vec!["-a", "--format", "{{.ID}}", "--no-trunc"],
                        )).into_command();
                        command
                            .stderr(Stdio::null())
                            .stdout(Stdio::piped());
                        debug!("{}: {}: ComposeBoundVolume: getting container ID: docker {:?}", service_name, archive_name, command.get_args().collect::<Vec<_>>());
                        match command.output() {
                            Ok(out) => {
                                if !out.status.success() {
                                    error!("{}: {}: ComposeBoundVolume: failed to get container ID", service_name, archive_name);
                                } else {
                                    let container_id = String::from_utf8_lossy(&out.stdout).trim().to_string();
                                    if container_id.is_empty() {
                                        error!("{}: {}: ComposeBoundVolume: container ID is empty", service_name, archive_name);
                                    } else {
                                        #[derive(Deserialize, Debug)]
                                        struct DockerContainerInspectOutput {
                                            #[serde(rename = "Mounts")]
                                            mounts: Vec<DockerContainerMount>,
                                        }

                                        #[derive(Deserialize, Debug)]
                                        struct DockerContainerMount {
                                            #[serde(rename = "Source")]
                                            source: String,
                                            #[serde(rename = "Destination")]
                                            destination: String,
                                        }

                                        let mut command = config.docker_command_with_context(DockerSubcommand::container(
                                            DockerContainerSubcommand::Inspect { container: container_id },
                                            vec!["--format", "json"],
                                        )).into_command();
                                        command
                                            .stdout(Stdio::piped());
                                        debug!("{}: {}: ComposeBoundVolume: inspecting container: docker {:?}", service_name, archive_name, command.get_args().collect::<Vec<_>>());
                                        let inspect_raw = command.output().expect("Failed to inspect container");
                                        let inspect = match serde_json::from_slice::<Vec<DockerContainerInspectOutput>>(&inspect_raw.stdout)?.into_iter().next() {
                                            Some(i) => i,
                                            None => {
                                                error!("{}: {}: ComposeBoundVolume: no mounts found in container inspect output", service_name, archive_name);
                                                continue;
                                            }
                                        };
                                        match inspect.mounts.into_iter().find(|m| m.destination == path.to_string_lossy()) {
                                            Some(mount) => {
                                                let host_path = mount.source;
                                                mounts.push(DockerBinding::new_ro(host_path, output));
                                                if let Some(filter) = filter {
                                                    excludes.push(filter.join(&archive_name));
                                                }
                                            }
                                            None => error!("{}: {}: ComposeBoundVolume: specified mount path is not a bound volume", service_name, archive_name),
                                        }
                                    }
                                }
                            }
                            Err(err) => {
                                error!("{}: {}: ComposeBoundVolume: failed to get container ID: {}", service_name, archive_name, err);
                            }
                        }
                    }
                    _ => todo!(),
                }
            }
        }

        backups.push(ResticBackup::with_excludes(
            PathBuf::from(config.restic_root()).join(&service_name),
            excludes,
        ));
    }

    mounts.push(DockerBinding::new_ro(
        config.intermediate_mount_override().unwrap_or(config.intermediate_path()),
        PathBuf::from(config.restic_root()),
    ));
    debug!("mountlist: {:#?}", mounts);

    // command.args([RESTIC_IMAGE, "sleep", "infinity"]);
    // command.arg(config.restic_image());
    // debug!("docker {}", command.get_args().map(|arg| format!("\"{}\"", arg.to_string_lossy())).collect::<Vec<_>>().join(" "));
    // command.spawn().unwrap().wait().unwrap();

    // get restic related env variables
    let mut env = vec![
        ("RESTIC_PASSWORD_FILE".to_owned(), "/restic_password".to_owned()),
        ("RESTIC_HOST".to_owned(), config.restic_host()),
    ];

    for (key, value) in std::env::vars() {
        if key == "RESTIC_PASSWORD_FILE" {
            continue;
        }
        if key.starts_with("RESTIC_") || key.starts_with("AWS_") {
            debug!("setting env var: {}=***", key);
            env.push((key, value));
        }
    }
    let mut options = vec!["--rm".to_owned(), "--name".to_owned(), config.restic_container_name(), "-d".to_owned()];
    // append env vars
    for (k, v) in &env {
        options.push("--env".to_owned());
        options.push(format!("{}={}", k, v));
    }

    // stop any existing container
    if config.docker_command_with_context(DockerSubcommand::stop(
            config.restic_container_name(),
            Vec::<String>::new(),
        ))
        .spawn_and_wait()?
        .success()
    {
        warn!("another container with the name {} has been found and stopped", config.restic_container_name());
        warn!("waiting 1 second for letting the daemon delete it...");
        std::thread::sleep(std::time::Duration::from_secs(1));
    }

    if !config.docker_command_with_context(
        DockerSubcommand::run(
            config.restic_image(),
            mounts,
            options,
            vec!["tini", "--", "sleep", "infinity"],
        ))
        .spawn_and_wait()?
        .success()
    {
        error!("failed to start restic container");
        return Ok(());
    }

    for backup in backups {
        let task = backup.into_task();

        let mut command = config.docker_command_with_context(DockerSubcommand::exec(
            config.restic_container_name(),
            task,
            vec!["-it"],
        )).into_command();
        if config.dry_run() {
            warn!("running in dry run mode, not actually uploading");
            command.arg("--dry-run");
        }
        info!("running restic backup task: {:?}", command.get_args().collect::<Vec<_>>());
        let exit = command.spawn().expect("failed to start restic backup").wait().expect("failed to wait for restic container to start");
        if !exit.success() {
            error!("restic backup failed: {}", exit);
        }
    }

    let mut command = config.docker_command_with_context(
        DockerSubcommand::stop("hoarder-backup", Vec::<String>::with_capacity(0))
    ).into_command();
    command.spawn().expect("failed to stop restic container").wait().expect("failed to wait for restic container to stop");
}

#[test]
fn test_config_dump() {
    let test = vec![
        Service {
            name: "test_service".to_owned(),
            compose_project: Some("different_compose".to_owned()),
            archives: vec![
                ArchiveOptions {
                    input: ArchiveInput::Docker(DockerInputType::ComposeNamedVolume {
                        name: "test_volume".to_owned(),
                        filter: Some(PathExclude(vec![PathBuf::from("ses")])),
                    }),
                    name: "data".to_owned(),
                },
            ],
        }
    ];

    println!("{}", serde_yaml::to_string(&test).unwrap());
}
