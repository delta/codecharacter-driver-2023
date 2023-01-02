use std::{
    fs::File,
    process::{Child, Command, Stdio},
};

use crate::{
    error::SimulatorError, handle_process, COMPILATION_MEMORY_LIMIT, COMPILATION_TIME_LIMIT,
    RUNTIME_MEMORY_LIMIT, RUNTIME_TIME_LIMIT,
};

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
    fn run(&self, stdin: File, stdout: File) -> Result<Child, SimulatorError> {
        let compile = Command::new("timeout")
            .args([
                "--signal=KILL",
                COMPILATION_TIME_LIMIT,
                "docker",
                "run",
                &format!("--memory={}", COMPILATION_MEMORY_LIMIT),
                &format!("--memory-swap={}", COMPILATION_MEMORY_LIMIT),
                "--cpus=1.5",
                "--rm",
                "--name",
                &format!("{}_java_compiler", self.game_id),
                "-v",
                format!(
                    "{}/Run.java:/player_code/Run.java",
                    self.current_dir.as_str()
                )
                .as_str(),
                "-v",
                format!("{}/run.jar:/player_code/run.jar", self.current_dir.as_str()).as_str(),
                "ghcr.io/delta/codecharacter-java-compiler:latest",
            ])
            .current_dir(&self.current_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| {
                SimulatorError::UnidentifiedError(format!(
                    "Couldnt spawn compilation command: {}",
                    err
                ))
            })?;

        let _ = handle_process(compile, true, SimulatorError::CompilationError)?;

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
                &format!("{}_java_runner", self.game_id),
                "-i",
                "-v",
                format!("{}/run.jar:/run.jar", self.current_dir.as_str()).as_str(),
                "ghcr.io/delta/codecharacter-java-runner:latest",
            ])
            .current_dir(&self.current_dir)
            .stdin(stdin)
            .stdout(stdout)
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| {
                SimulatorError::UnidentifiedError(format!(
                    "Couldnt spawn the java runner process: {}",
                    err
                ))
            })
    }
}

impl Drop for Runner {
    fn drop(&mut self) {
        Command::new("docker")
            .args([
                "stop",
                &format!("{}_java_compiler", self.game_id),
                &format!("{}_java_runner", self.game_id)
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .ok();
    }
}

impl Executable for Runner {}
