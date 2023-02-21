use std::fs::File;

use std::env;
use std::os::fd::RawFd;
use std::os::linux::process::CommandExt;
use std::process::{Command, Stdio};

use crate::error::SimulatorError;

pub struct Simulator {
    game_id: String,
}

impl Simulator {
    pub fn new(game_id: String) -> Self {
        Simulator { game_id }
    }

    pub fn run_pvp(
        &self,
        stdin: File,
        stdout: File,
        p1_r: RawFd,
        p1_w: RawFd,
        p2_r: RawFd,
        p2_w: RawFd,
    ) -> Result<std::process::Child, SimulatorError> {
        Command::new("/home/ram/Desktop/codecharacter-2023/codecharacter-simulator-2023/build/bin/main")
            .args([
                // "run",
                // &format!("--memory={}", "100m"),
                // &format!(
                //     "--memory-swap={}",
                //     "100m"
                // ),
                // "--cpus=1",
                // "--ulimit",
                // &format!(
                //     "cpu={}:{}",
                //     "100",
                //     "100"
                // ),
                // "--rm",
                // "--name",
                // &format!("{}_simulator", self.game_id),
                // "-i",
                // "pvp_sim",
                "--type=PvP",
                &format!("p1_in={p1_r}"),
                &format!("p1_out={p1_w}"),
                &format!("p2_in={p2_r}"),
                &format!("p2_out={p2_w}"),
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

    pub fn run(&self, stdin: File, stdout: File) -> Result<std::process::Child, SimulatorError> {
        Command::new("docker")
            .args([
                "run",
                &format!("--memory={}", "100m"),
                &format!(
                    "--memory-swap={}",
                    "100m"
                ),
                "--cpus=1",
                "--ulimit",
                &format!(
                    "cpu={}:{}",
                    "10",
                    "10"
                ),
                "--rm",
                "--name",
                &format!("{}_simulator", self.game_id),
                "-i",
                "ghcr.io/delta/codecharacter-simulator:latest",
                "--type=Normal",
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
