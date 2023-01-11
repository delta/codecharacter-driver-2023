use std::fs::File;

use std::os::linux::process::CommandExt;
use std::process::{Command, Stdio};

use crate::error::SimulatorError;
use crate::{RUNTIME_MEMORY_LIMIT, RUNTIME_TIME_LIMIT};

use super::Run;

pub struct Simulator {
    game_id: String,
}

impl Simulator {
    pub fn new(game_id: String) -> Self {
        Simulator { game_id }
    }
}

impl Run for Simulator {
    fn run(&self, stdin: File, stdout: File) -> Result<std::process::Child, SimulatorError> {
        Command::new("docker")
            .args([
                "run",
                &format!("--memory={RUNTIME_MEMORY_LIMIT}"),
                &format!("--memory-swap={RUNTIME_MEMORY_LIMIT}"),
                "--cpus=1",
                "--ulimit",
                &format!("cpu={RUNTIME_TIME_LIMIT}:{RUNTIME_TIME_LIMIT}"),
                "--rm",
                "--name",
                &format!("{}_simulator", self.game_id),
                "-i",
                "ghcr.io/delta/codecharacter-simulator:latest",
            ])
            .create_pidfd(true)
            .stdin(stdin)
            .stdout(stdout)
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| {
                SimulatorError::UnidentifiedError(format!(
                    "Couldnt spawn the simulator process: {err}"
                ))
            })
    }
}
