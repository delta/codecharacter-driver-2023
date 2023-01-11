use std::{fs::File, process::Child};

use crate::error::SimulatorError;

pub mod cpp;
pub mod java;
pub mod py;
pub mod simulator;

pub trait Run {
    fn run(&self, stdin: File, stdout: File) -> Result<Child, SimulatorError>;
}
