use std::{
    fs::{File, canonicalize},
    process::{Child, Command, Stdio}, path::PathBuf, os::linux::process::CommandExt,
};

use crate::{
    error::SimulatorError, COMPILATION_MEMORY_LIMIT,
    RUNTIME_MEMORY_LIMIT,
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
        let cpu_timeout = canonicalize(PathBuf::from("./cputimeout.sh")).unwrap().into_os_string();

        let compile = Command::new("docker")
            .args([
                "run",
                &format!("--memory={COMPILATION_MEMORY_LIMIT}"),
                &format!("--memory-swap={COMPILATION_MEMORY_LIMIT}"),
                "--cpus=2",
                "--rm",
                "--name",
                &format!("{}_cpp_compiler", self.game_id),
                "-v",
                format!("{}/run.cpp:/player_code/run.cpp", self.current_dir.as_str()).as_str(),
                "-v",
                format!("{}/run:/player_code/run", self.current_dir.as_str()).as_str(),
                "ghcr.io/delta/codecharacter-cpp-compiler:latest",
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

        let out = compile.wait_with_output().unwrap();
        if !out.status.success() {
            let stderr = String::from_utf8(out.stderr).unwrap();
            return Err(SimulatorError::CompilationError(stderr));
        }

        // let _ = handle_process(compile, true, SimulatorError::CompilationError)?;

        Command::new(cpu_timeout)
            .args([
                "1",
                "docker",
                "run",
                &format!("--memory={RUNTIME_MEMORY_LIMIT}"),
                &format!("--memory-swap={RUNTIME_MEMORY_LIMIT}"),
                "--cpus=1",
                "--rm",
                "--name",
                &format!("{}_cpp_runner", self.game_id),
                "-i",
                "-v",
                format!("{}/run:/player_code", self.current_dir.as_str()).as_str(),
                "ghcr.io/delta/codecharacter-cpp-runner:latest",
            ])
            .current_dir(&self.current_dir)
            .create_pidfd(true)
            .stdin(stdin)
            .stdout(stdout)
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| {
                SimulatorError::UnidentifiedError(format!(
                    "Couldnt spawn the C++ runner process: {err}"))
            })
    }
}

impl Drop for Runner {
    fn drop(&mut self) {
        println!("Removing the cpp runner");
        Command::new("docker")
            .args([
                "stop",
                &format!("{}_cpp_compiler", self.game_id),
                &format!("{}_cpp_runner", self.game_id),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok();
    }
}

impl Executable for Runner {}
