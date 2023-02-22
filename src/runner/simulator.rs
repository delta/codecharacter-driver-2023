use std::fs::File;

use std::env;
use std::os::linux::process::CommandExt;
use std::process::{Command, Stdio};

use crate::error::SimulatorError;

use super::Runnable;

pub struct Simulator {
    game_id: String,
}

impl Simulator {
    pub fn new(game_id: String) -> Self {
        Simulator { game_id }
    }
}

impl Runnable for Simulator {
    fn run(&self, stdin: File, stdout: File) -> Result<std::process::Child, SimulatorError> {
        Command::new("docker")
            .args([
                "run",
                &format!("--memory={}", env::var("RUNTIME_MEMORY_LIMIT").unwrap()),
                &format!(
                    "--memory-swap={}",
                    env::var("RUNTIME_MEMORY_LIMIT").unwrap()
                ),
                "--cpus=1",
                "--ulimit",
                &format!(
                    "cpu={}:{}",
                    env::var("RUNTIME_TIME_LIMIT").unwrap(),
                    env::var("RUNTIME_TIME_LIMIT").unwrap()
                ),
                "--rm",
                "--name",
                &format!("{}_simulator", self.game_id),
                "-i",
                &env::var("SIMULATOR_IMAGE").unwrap(),
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
