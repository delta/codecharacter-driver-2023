use std::{
    fs::{File, canonicalize},
    path::PathBuf,
    process::{Child, Command, Stdio},
};

use crate::{
    error::SimulatorError, handle_process, COMPILATION_MEMORY_LIMIT, COMPILATION_TIME_LIMIT,
    RUNTIME_MEMORY_LIMIT, RUNTIME_TIME_LIMIT, Execute,
};

pub struct Runner {
    current_dir: String,
}

impl Runner {
    pub fn new(current_dir: String) -> Self {
        Runner { current_dir }
    }
}

impl Execute for Runner {
    fn run(&self, stdin: File, stdout: File, game_id: String) -> Result<Child, SimulatorError> {
        let cpu_timeout = canonicalize(PathBuf::from("./cputimeout.sh")).unwrap().into_os_string();

        let compile = Command::new(cpu_timeout.clone())
            .args([
                COMPILATION_TIME_LIMIT,
                "docker",
                "run",
                &format!("--memory={}", COMPILATION_MEMORY_LIMIT),
                &format!("--memory-swap={}", COMPILATION_MEMORY_LIMIT),
                "--cpus=2",
                "--rm",
                "--name",
                format!("{}_cpp_compiler", game_id).as_str(),
                "-v",
                format!("{}/run.cpp:/player_code/run.cpp", self.current_dir.as_str()).as_str(),
                "-v",
                format!("{}/run:/player_code/run", self.current_dir.as_str()).as_str(),
                "ghcr.io/delta/codecharacter-cpp-compiler:latest",
            ])
            .current_dir(&self.current_dir.to_owned())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| {
                SimulatorError::UnidentifiedError(format!(
                    "Couldnt spawn compilation command: {}",
                    err
                ))
            })?;

        let _ = handle_process(compile, true, |x| SimulatorError::CompilationError(x))?;

        Command::new(cpu_timeout.clone())
            .args([
                RUNTIME_TIME_LIMIT,
                "docker",
                "run",
                &format!("--memory={}", RUNTIME_MEMORY_LIMIT),
                &format!("--memory-swap={}", RUNTIME_MEMORY_LIMIT),
                "--cpus=1",
                "--rm",
                "--name",
                format!("{}_cpp_runner", game_id).as_str(),
                "-i",
                "-v",
                format!("{}/run:/player_code", self.current_dir.as_str()).as_str(),
                "ghcr.io/delta/codecharacter-cpp-runner:latest",
            ])
            .current_dir(&self.current_dir.to_owned())
            .stdin(stdin)
            .stdout(stdout)
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| {
                SimulatorError::UnidentifiedError(format!(
                    "Couldnt spawn the C++ runner process: {}",
                    err
                ))
            })
    }
}
