use std::{
    fs::File,
    os::linux::process::CommandExt,
    process::{Command, Stdio},
};

use crate::{error::SimulatorError, RUNTIME_MEMORY_LIMIT, RUNTIME_TIME_LIMIT};

use super::Runnable;

pub struct Runner {
    current_dir: String,
    game_id: String,
}

impl Runner {
    pub fn new(current_dir: String, game_id: String) -> Self {
        Runner {
            current_dir,
            game_id,
        }
    }
}

impl Runnable for Runner {
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
                &format!("{}_python_runner", self.game_id),
                "-i",
                "-v",
                format!("{}/run.py:/player_code/run.py", self.current_dir.as_str()).as_str(),
                "ghcr.io/delta/codecharacter-python-runner:latest",
            ])
            .create_pidfd(true)
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
