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
    player_dir: String,
}

impl Runner {
    pub fn new(current_dir: String, game_id: String, player_dir: String) -> Self {
        Runner {
            current_dir,
            game_id,
            player_dir,
        }
    }
}

impl Runnable for Runner {
    fn run(&self, stdin: File, stdout: File, game_type: GameType) -> Result<Child, SimulatorError> {
        // let compile = Command::new("docker")
        //     .args([
        //         "run",
        //         &format!("--memory={}", "300m"),
        //         &format!(
        //             "--memory-swap={}",
        //             "300m"
        //         ),
        //         "--cpus=2",
        //         "--ulimit",
        //         &format!(
        //             "cpu={}:{}",
        //            "5",
        //            "5"
        //         ),
        //         "--rm",
        //         "--name",
        //         &format!("{}_{}_cpp_compiler", self.game_id, self.player_dir),
        //         "-v",
        //         format!(
        //             "{}/{}:/player_code/",
        //             self.current_dir.as_str(),
        //             self.player_dir.as_str()
        //         )
        //         .as_str(),
        //         "ghcr.io/delta/codecharacter-cpp-compiler:latest"
        //     ])
        //     .current_dir(&self.current_dir)
        //     .stdout(Stdio::null())
        //     .stderr(Stdio::piped())
        //     .spawn()
        //     .map_err(|err| {
        //         SimulatorError::UnidentifiedError(format!(
        //             "Couldnt spawn compilation command: {err}"
        //         ))
        //     })?;

        // let out = compile.wait_with_output().map_err(|err| {
        //     SimulatorError::UnidentifiedError(format!(
        //         "Unable to wait for compilation to finish, {err}"
        //     ))
        // })?;

        // if !out.status.success() {
        //     let stderr = String::from_utf8(out.stderr).unwrap();
        //     return Err(SimulatorError::CompilationError(stderr));
        // }

        Command::new("/home/ram/Desktop/codecharacter-2023/codecharacter-driver-2023/player_code/cpp/run")
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
                //     "10",
                //     "10"
                // ),
                // "--rm",
                // "--name",
                // &format!("{}_{}_cpp_runner", self.game_id, self.player_dir),
                // "-i",
                // "-v",
                // &format!(
                //     "{}/{}/run:/player_code",
                //     self.current_dir.as_str(),
                //     self.player_dir.as_str()
                // ),
                // "ghcr.io/delta/codecharacter-cpp-runner:latest",
                // pass the type of game we want to execute
                &game_type.to_string(),
            ])
            .current_dir(&self.current_dir)
            .create_pidfd(true)
            .stdin(stdin)
            .stdout(stdout)
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| {
                SimulatorError::UnidentifiedError(format!(
                    "Couldnt spawn the C++ runner process: {err}"
                ))
            })
    }
}
