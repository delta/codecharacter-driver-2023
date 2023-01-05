use std::fs::{File, canonicalize};

use std::os::linux::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};




use crate::{RUNTIME_MEMORY_LIMIT};
use crate::error::SimulatorError;

use super::{Executable, Run};

pub struct Simulator {
    game_id: String
}

impl Simulator {
    pub fn new(game_id: String) -> Self {
        Simulator { game_id }
    }
}

impl Run for Simulator {
    fn run(&self, stdin: File, stdout: File) -> Result<std::process::Child, SimulatorError> {
        let cpu_timeout = canonicalize(PathBuf::from("./cputimeout.sh")).unwrap().into_os_string();

        Command::new(cpu_timeout)
            .args([
                "100",
                "docker",
                "run",
                &format!("--memory={RUNTIME_MEMORY_LIMIT}"),
                &format!("--memory-swap={RUNTIME_MEMORY_LIMIT}"),
                "--cpus=1",
                "--rm", 
                "--name",
                &format!("{}_simulator", self.game_id),
                "-i",
                "exit_image",
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

impl Drop for Simulator {
    fn drop(&mut self) {
        println!("Removing simulator");
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

impl Executable for Simulator {}
