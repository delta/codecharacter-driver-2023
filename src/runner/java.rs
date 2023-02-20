use std::{
    env,
    fs::File,
    os::linux::process::CommandExt,
    process::{Child, Command, Stdio},
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
    fn run(&self, stdin: File, stdout: File, game_type: GameType) -> Result<Child, SimulatorError> {
        let compile = Command::new("docker")
            .args([
                "run",
                &format!("--memory={}", env::var("COMPILATION_MEMORY_LIMIT").unwrap()),
                &format!(
                    "--memory-swap={}",
                    env::var("COMPILATION_MEMORY_LIMIT").unwrap()
                ),
                "--cpus=1.5",
                "--ulimit",
                &format!(
                    "cpu={}:{}",
                    env::var("COMPILATION_TIME_LIMIT").unwrap(),
                    env::var("COMPILATION_TIME_LIMIT").unwrap()
                ),
                "--rm",
                "--name",
                &format!("{}_java_compiler", self.game_id),
                "-v",
                format!(
                    "{}/{}.java:/player_code/Run.java",
                    self.current_dir.as_str(),
                    self.file_name.as_str(),
                )
                .as_str(),
                "-v",
                format!(
                    "{}/{}.jar:/player_code/run.jar",
                    self.current_dir.as_str(),
                    self.file_name.as_str()
                )
                .as_str(),
                "ghcr.io/delta/codecharacter-java-compiler:latest",
                &game_type.to_string(),
            ])
            .current_dir(&self.current_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| {
                SimulatorError::UnidentifiedError(format!(
                    "Couldnt spawn compilation command: {err}"
                ))
            })?;

        let out = compile.wait_with_output().map_err(|err| {
            SimulatorError::UnidentifiedError(format!(
                "Unable to wait for compilation to finish, {err}"
            ))
        })?;

        if !out.status.success() {
            let stderr = String::from_utf8(out.stderr).unwrap();
            return Err(SimulatorError::CompilationError(stderr));
        }

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
                &format!("{}_java_runner", self.game_id),
                "-i",
                "-v",
                format!(
                    "{}/{}.jar:/run.jar",
                    self.current_dir.as_str(),
                    self.file_name.as_str()
                )
                .as_str(),
                "ghcr.io/delta/codecharacter-java-runner:latest",
            ])
            .create_pidfd(true)
            .current_dir(&self.current_dir)
            .stdin(stdin)
            .stdout(stdout)
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| {
                SimulatorError::UnidentifiedError(format!(
                    "Couldnt spawn the java runner process: {err}"
                ))
            })
    }
}
