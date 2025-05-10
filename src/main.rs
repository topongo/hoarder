use archive::{ArchiveInput, ArchiveOptions};
use config::FullConfig;
use indicatif::HumanBytes;
use log::{debug, error, info, warn};
use service::Service;
use std::{fs::File, io::{BufReader, BufWriter, Read, Write}, path::PathBuf, process::Stdio};
use serde::Deserialize;

mod config;
mod service;
mod archive;
mod task;
mod docker;

use task::ShellTask;
use docker::DockerInputType;

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

fn main() {
    pretty_env_logger::init();

    let config = std::fs::read_to_string("config.yaml").expect("Failed to read config file");
    let FullConfig { services, config } = serde_yaml::from_str(&config).expect("Failed to parse config file");

    let mut mountlist = Vec::new();

    info!("Backup summary:");
    for service in &services {
        info!("- {}:", service.name);
        for archive in &service.archives {
            info!("  - {}: {:?}", archive.name, archive.input);
        }
    }
    info!("");

    for service in services {
        debug!("{}: service: {:?}", service.name, service);
        let Service { archives, compose_project, name: service_name } = service;
        let compose_project = compose_project.unwrap_or(service_name.clone());
        for archive in archives {
            debug!("{}: {}: archive: {:?}", service_name, compose_project, archive);
            let ArchiveOptions { input, name: archive_name } = archive;
            match input {
                ArchiveInput::Docker(docker_input) => match docker_input {
                    DockerInputType::ExecStdout { service, task, ext } => {
                        info!("{}: {}: using mode: ExecStdout", service_name, archive_name);
                        let mut command = std::process::Command::new("docker");
                        command.args(["compose", "-p", &compose_project, "exec", "-i", &service]);
                        command.args(task.args());
                        let output_path = PathBuf::from(config.base_path()).join(&service_name);
                        std::fs::create_dir_all(&output_path).unwrap();
                        let output_name = format!("{}.{}", archive_name, ext);
                        let output_file = output_path.join(output_name);
                        debug!("{}: {}: ExecStdout: output file: {:?}", service_name, archive_name, output_file);

                        let output = File::create(&output_file).unwrap();
                        command
                            .stderr(std::process::Stdio::piped())
                            .stdout(Stdio::piped());
                        debug!("{}: {}: ExecStdout: executing command: {:?}", service_name, archive_name, command.get_args().collect::<Vec<_>>());
                        let mut handle = command.spawn().expect("Failed to start task");
                        let stdout = handle.stdout.take().expect("Failed to get stdout");
                        let mut proxy = if config.dry_run {
                            warn!("{}: {}: dry run mode, not writing to file", service_name, archive_name);
                            SpinnerWriter {
                                output: BufWriter::new(Box::new(std::io::sink())),
                                input: BufReader::new(stdout),
                                bytes_written: 0,
                                bar: indicatif::ProgressBar::new_spinner(),
                            }
                        } else {
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
                        let output = PathBuf::from(config.restic_base_path()).join(&service_name).join(&archive_name);
                        // ensure global volume exists
                        let mut command = std::process::Command::new("docker");
                        command
                            .args(["volume", "inspect", &global_volume_name])
                            .stderr(Stdio::null())
                            .stdout(Stdio::null());
                        debug!("{}: {}: ComposeNamedVolume: inspecting volume: docker {:?}", service_name, archive_name, command.get_args().collect::<Vec<_>>());
                        let status = command.status().expect("Failed to check volume");
                        if !status.success() {
                            error!("{}: {}: ComposeNamedVolume: volume {} does not exist", service_name, archive_name, global_volume_name);
                        } else {
                            mountlist.push(format!("{}:{}", global_volume_name, output.display()))
                        }
                    }
                    DockerInputType::ComposeBoundVolume { service, path, filter } => {
                        info!("{}: {}: using mode: ComposeBoundVolume", service_name, archive_name);
                        let output = PathBuf::from(config.restic_base_path()).join(&service_name).join(&archive_name);
                        // find the bound volume inside the service
                        let mut command = std::process::Command::new("docker");
                        command
                            .args(["compose", "-p", &compose_project, "ps", "-a", &service, "--format", "{{.ID}}", "--no-trunc"])
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

                                        let mut command = std::process::Command::new("docker");
                                        command
                                            .args(["container", "inspect", "--format", "json"])
                                            .arg(&container_id)
                                            .stdout(Stdio::piped());
                                        debug!("{}: {}: ComposeBoundVolume: inspecting container: docker {:?}", service_name, archive_name, command.get_args().collect::<Vec<_>>());
                                        let inspect_raw = command.output().expect("Failed to inspect container");
                                        let inspect = serde_json::from_slice::<Vec<DockerContainerInspectOutput>>(&inspect_raw.stdout).expect("Failed to parse container inspect output").into_iter().next().unwrap();
                                        match inspect.mounts.into_iter().find(|m| m.destination == path.to_string_lossy()) {
                                            Some(mount) => {
                                                let host_path = mount.source;
                                                mountlist.push(format!("{}:{}", host_path, output.display()));
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
    }

    debug!("mountlist: {:#?}", mountlist);
    let mut command = std::process::Command::new("docker");
    command
        .args(["run", "--rm"])
        .arg("-v")
        .arg(format!("{}:{}", config.base_path(), config.restic_base_path()));
    for m in mountlist {
        command.args(["-v", &m]);
    }
    // command.args([RESTIC_IMAGE, "sleep", "infinity"]);
    command.arg(config.restic_image());
    debug!("docker {}", command.get_args().map(|arg| format!("\"{}\"", arg.to_string_lossy())).collect::<Vec<_>>().join(" "));
    // command.spawn().unwrap().wait().unwrap();
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
                        filter: Some(PathFilter::Exclude(vec!["ses".to_owned()])),
                    }),
                    name: "data".to_owned(),
                },
            ],
        }
    ];

    println!("{}", serde_yaml::to_string(&test).unwrap());
}

