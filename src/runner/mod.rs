use std::{fs::File, process::Child};

use crate::{error::SimulatorError, request::Language};

pub mod cpp;
pub mod java;
pub mod py;
pub mod simulator;

pub enum GameType {
    NormalGame,
    PvPGame,
}

impl GameType {
    pub fn file_name(&self, language: Language) -> &str {
        match self {
            GameType::PvPGame => match language {
                Language::CPP | Language::PYTHON => "runpvp",
                Language::JAVA => "RunPvp",
            },
            GameType::NormalGame => match language {
                Language::CPP | Language::PYTHON => "run",
                Language::JAVA => "Run",
            },
        }
    }
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
