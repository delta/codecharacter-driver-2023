use std::fs::File;

use std::process::{Command, Stdio};

use crate::{RUNTIME_TIME_LIMIT, RUNTIME_MEMORY_LIMIT};
use crate::error::SimulatorError;

use super::Executable;

pub struct Simulator {
    game_id: String
}

impl Simulator {
    pub fn new(game_id: String) -> Self {
        Simulator { game_id }
    }
}

impl Executable for Simulator {
    fn run(&self, stdin: File, stdout: File) -> Result<std::process::Child, SimulatorError> {
        Command::new("timeout")
            .args([
                "--signal=KILL",
                RUNTIME_TIME_LIMIT,
                "docker",
                "run",
                &format!("--memory={}", RUNTIME_MEMORY_LIMIT),
                &format!("--memory-swap={}", RUNTIME_MEMORY_LIMIT),
                "--cpus=1",
                "--rm",
                "--name",
                &format!("{}_simulator", self.game_id),
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

impl Drop for Simulator {
    fn drop(&mut self) {
        Command::new("docker")
            .args([
                "stop",
                &format!("{}_simulator", self.game_id),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .ok();
    }
}
