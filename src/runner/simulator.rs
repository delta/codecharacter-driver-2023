use std::fs::File;

use std::os::linux::process::CommandExt;
use std::process::{Command, Stdio};

use std::env;

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
        p1_r: String,
        p1_w: String,
        p2_r: String,
        p2_w: String,
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
                "--name",
                "--rm",
                &format!("{}_simulator", self.game_id),
                "-i",
                "-v",
                &format!("/tmp/{}:/tmp/{}", self.game_id, self.game_id),
                &env::var("SIMULATOR_IMAGE").unwrap(),
                "--type=PvP",
                &format!("p1_in={p1_r}"),  //p1_in
                &format!("p1_out={p1_w}"), // p3_in
                &format!("p2_in={p2_r}"),  // p2_in
                &format!("p2_out={p2_w}"), // p4_in
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
