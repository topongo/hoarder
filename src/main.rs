use log::{error, info, debug};
use std::{fs::File, io::Read, path::PathBuf, process::Stdio};
use serde::{Serialize, Deserialize};

static BASE_PATH: &str = "./output";
static RESTIC_BASE_PATH: &str = "/backup";
static RESTIC_IMAGE: &str = "test";

#[derive(Serialize, Deserialize, Debug)]
#[serde(transparent)]
struct ShellTask {
    args: Vec<String>,
}

impl ShellTask {
    fn new(args: Vec<impl ToString>) -> Self {
        ShellTask { args: args.into_iter().map(|arg| arg.to_string()).collect() }
    }

    fn autosplit(args: impl ToString) -> Self {
        let args = args.to_string();
        if args.contains('"') {
            panic!("autosplit can't be used on a string containing quoted arguments!");
        }
        Self {
            args: args.split_whitespace().map(|arg| arg.to_string()).collect(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
enum OutputType {
    StdOut(ShellTask),
    Directory(PathBuf),
    File(PathBuf),
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
enum ArchiveMode {
    None,
    Single(ShellTask),
    Multi(Vec<ShellTask>),
}

#[derive(Serialize, Deserialize, Debug)]
enum DockerVolumeName {
    Compute(ShellTask),
    Static(String),
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
enum PathFilter {
    Include(Vec<String>),
    Exclude(Vec<String>),
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "docker_type")]
enum DockerInputType {
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
        command: ShellTask,
        ext: String,
    }
}

#[derive(Serialize, Deserialize, Debug)]
enum ArchiveInput {
    Docker(DockerInputType),
    // Directory {
    //     path: PathBuf,
    //     prepare: Vec<ShellTask>,
    // },
}

#[derive(Serialize, Deserialize, Debug)]
struct ArchiveOptions {
    input: ArchiveInput,
    // output: OutputType,
    // mode: ArchiveMode,
    name: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct Service {
    name: String,
    archives: Vec<ArchiveOptions>,
    compose_project: Option<String>,
}

fn main() {
    pretty_env_logger::init();

    // load yml from disk
    let data = std::fs::read_to_string("config.yaml").expect("Unable to read file");
    let test: Vec<Service> = serde_yaml::from_str(&data).expect("Unable to parse YAML");
    // let test = Service {
    //     name: "joplin".to_owned(),
    //     compose_project: None,
    //     archives: vec![
    //         ArchiveOptions { 
    //             input: ArchiveInput::Docker(DockerInputType::ExecStdout { 
    //                 service: "db".to_string(), 
    //                 command: ShellTask::autosplit("pgdump -U postgres")
    //             }),
    //             name: "db".to_string(),
    //         },
    //     ],
    // };

    println!("{}", serde_yaml::to_string(&test).unwrap());

    let dry_run = true;

    let mut mountlist = vec![];

    let services = test;
    for service in services {
        let Service { archives, compose_project, name: service_name } = service;
        let compose_project = compose_project.unwrap_or(service_name.clone());
        for archive in archives {
            let ArchiveOptions { input, name: archive_name } = archive;
            match input {
                ArchiveInput::Docker(docker_input) => match docker_input {
                    DockerInputType::ExecStdout { service, command, ext } => {
                        let args = ["compose", "-p", &compose_project, "exec", &service]
                            .iter()
                            .map(|arg| arg.to_string())
                            .chain(command.args.into_iter())
                            .collect::<Vec<String>>();
                        let output_path = PathBuf::from(BASE_PATH).join(&service_name);
                        std::fs::create_dir_all(&output_path).unwrap();
                        let output_name = format!("{}.{}", archive_name, ext);
                        let output_file = output_path.join(output_name);

                        let output = File::create(&output_file).unwrap();
                        let mut command = std::process::Command::new("docker");
                        command
                            .args(args)
                            .stderr(std::process::Stdio::piped())
                            .stdout(output);
                        if !dry_run {
                            let mut handle = command.spawn().expect("Failed to start task");
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
                        } else {
                            info!("dryrun: executing command: {:?} {:?}", command.get_program(), command.get_args().collect::<Vec<_>>());
                            info!("dryrun: output file: {:?}", output_file.display());
                        }
                    }
                    DockerInputType::ComposeNamedVolume { name, filter } => {
                        let global_volume_name = format!("{compose_project}_{name}");
                        debug!("{}: {}: ComposeNamedVolume: using canonical volume name: {}", service_name, archive_name, global_volume_name);
                        let output = PathBuf::from(RESTIC_BASE_PATH).join(&service_name).join(&archive_name);
                        // ensure global volume exists
                        let mut command = std::process::Command::new("docker");
                        command
                            .args(["volume", "inspect", &global_volume_name])
                            .stderr(Stdio::null())
                            .stdout(Stdio::null());
                        let status = command.status().expect("Failed to check volume");
                        if !status.success() {
                            error!("{}: {}: ComposeNamedVolume: volume {} does not exist", service_name, archive_name, global_volume_name);
                        } else {
                            mountlist.push(format!("{}:{}", global_volume_name, output.display()))
                        }
                    }
                    DockerInputType::ComposeBoundVolume { service, path, filter } => {
                        let output = PathBuf::from(RESTIC_BASE_PATH).join(&service_name).join(&archive_name);
                        // find the bound volume inside the service
                        let mut command = std::process::Command::new("docker");
                        command
                            .args(["compose", "-p", &compose_project, "ps", &service, "--format", "{{.ID}}", "--no-trunc"])
                            .stderr(Stdio::null())
                            .stdout(Stdio::piped());
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
        .arg(format!("{}:{}", BASE_PATH, RESTIC_BASE_PATH));
    for m in mountlist {
        command.args(["-v", &m]);
    }
    // command.args([RESTIC_IMAGE, "sleep", "infinity"]);
    command.arg(RESTIC_IMAGE);
    debug!("{:?}", command.get_args())
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
