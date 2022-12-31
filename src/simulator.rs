use std::{
    fs::{File, canonicalize},
    path::PathBuf,
    process::{Command, Stdio},
};

use crate::{
    Execute,
    error::SimulatorError, SIMULATOR_TIME_LIMIT, SIMULATOR_MEMORY_LIMIT
};

pub struct Simulator {}

impl Execute for Simulator {
    fn run(&self, stdin: File, stdout: File, game_id: String) -> Result<std::process::Child, SimulatorError> {
        let cpu_timeout = canonicalize(PathBuf::from("./cputimeout.sh")).unwrap().into_os_string();

        Command::new(cpu_timeout.clone())
            .args([
                SIMULATOR_TIME_LIMIT,
                "docker",
                "run",
                &format!("--memory={}", SIMULATOR_MEMORY_LIMIT),
                &format!("--memory-swap={}", SIMULATOR_MEMORY_LIMIT),
                "--cpus=1",
                "--rm",
                "--name",
                format!("{}_simulator", game_id).as_str(),
                "-i",
                "ghcr.io/delta/codecharacter-simulator:latest",
            ])
            .stdin(stdin)
            .stdout(stdout)
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| {
                SimulatorError::UnidentifiedError(format!(
                    "Couldnt spawn the simulator process: {}",
                    err
                ))
            })
    }
}
