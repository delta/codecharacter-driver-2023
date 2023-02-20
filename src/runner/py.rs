use std::{
    env,
    fs::File,
    os::linux::process::CommandExt,
    process::{Command, Stdio},
};

use crate::error::SimulatorError;

use super::{GameType, Runnable};

pub struct Runner {
    current_dir: String,
    game_id: String,
    file_name: String,
}

impl Runner {
    pub fn new(current_dir: String, game_id: String, file_name: String) -> Self {
        Runner {
            current_dir,
            game_id,
            file_name,
        }
    }
}

impl Runnable for Runner {
    fn run(
        &self,
        stdin: File,
        stdout: File,
        game_type: GameType,
    ) -> Result<std::process::Child, SimulatorError> {
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
                &format!("{}_python_runner", self.game_id),
                "-i",
                "-v",
                format!("{}/:/player_code/", self.current_dir.as_str()).as_str(),
                &env::var("PYTHON_RUNNER_IMAGE").unwrap(),
                &game_type.to_string(),
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
