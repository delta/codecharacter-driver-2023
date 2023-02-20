use std::{fs::File, process::Child};

use crate::error::SimulatorError;

pub mod cpp;
pub mod java;
pub mod py;
pub mod simulator;

pub enum GameType {
    NormalGame,
    PvPGame,
}

impl ToString for GameType {
    fn to_string(&self) -> String {
        match self {
            GameType::NormalGame => "normal".to_owned(),
            GameType::PvPGame => "pvp".to_owned(),
        }
    }
}

pub trait Runnable {
    fn run(&self, stdin: File, stdout: File, game_type: GameType) -> Result<Child, SimulatorError>;
}
