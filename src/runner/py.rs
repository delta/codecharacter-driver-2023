use std::{
    fs::File,
    process::{Command, Stdio},
};

use crate::{error::SimulatorError, RUNTIME_MEMORY_LIMIT, RUNTIME_TIME_LIMIT};

use super::{Executable, Run};

pub struct Runner {
    current_dir: String,
    game_id: String,
}

impl Runner {
    pub fn new(current_dir: String, game_id: String) -> Self {
        Runner { current_dir, game_id }
    }
}

impl Run for Runner {
    fn run(&self, stdin: File, stdout: File) -> Result<std::process::Child, SimulatorError> {
        Command::new("timeout")
            .args([
                "--signal=KILL",
                RUNTIME_TIME_LIMIT,
                "docker",
                "run",
                &format!("--memory={RUNTIME_MEMORY_LIMIT}"),
                &format!("--memory-swap={RUNTIME_MEMORY_LIMIT}"),
                "--cpus=1",
                "--rm",
                "--name",
                &format!("{}_python_runner", self.game_id),
                "-i",
                "-v",
                format!("{}/run.py:/player_code/run.py", self.current_dir.as_str()).as_str(),
                "ghcr.io/delta/codecharacter-python-runner:latest",
            ])
            .current_dir(&self.current_dir)
            .stdin(stdin)
            .stdout(stdout)
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| {
                SimulatorError::UnidentifiedError(format!(
                    "Couldnt spawn the python runner process: {err}"
                ))
            })
    }
}

impl Drop for Runner {
    fn drop(&mut self) {
        Command::new("docker")
            .args([
                "stop",
                &format!("{}_python_runner", self.game_id)
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok();
    }
}

impl Executable for Runner {}
