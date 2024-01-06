use std::{
    fs::File,
    io::{BufWriter, Write},
};


use fs_extra::dir::CopyOptions;
use log::info;

use crate::{
    create_error_response, error,
    request::{GameParameters, NormalGameRequest, PlayerCode, PvPGameRequest, Language},
    response::{self, GameStatus}, game_dir::GameDir, runner::GameType,
};

pub fn copy_dir_all(
    src: impl AsRef<std::path::Path>,
    dst: impl AsRef<std::path::Path>,
) -> std::io::Result<()> {
    let opt = CopyOptions::new();
    info!("Copying dir from {:?} to {:?}", src.as_ref(), dst.as_ref());
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            fs_extra::dir::copy(entry.path(), dst.as_ref(), &opt)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("{:?}", e)))?;
        } else {
            std::fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }

    Ok(())
}

pub fn send_initial_parameters<'a>(
    mut writer: BufWriter<&'a File>,
    game_parameters: &'a GameParameters,
) -> BufWriter<&'a File> {
    writer
        .write_all(format!("{} {}\n", "5", "1000").as_bytes())
        .unwrap();
    writer
        .write_all(format!("{}\n", game_parameters.attackers.len()).as_bytes())
        .unwrap();
    for attacker in &game_parameters.attackers {
        writer
            .write_all(
                format!(
                    "{} {} {} {} {} {}\n",
                    attacker.hp,
                    attacker.range,
                    attacker.attack_power,
                    attacker.speed,
                    attacker.price,
                    "0",
                )
                .as_bytes(),
            )
            .unwrap();
    }
    writer
        .write_all(format!("{}\n", game_parameters.defenders.len()).as_bytes())
        .unwrap();
    for defender in &game_parameters.defenders {
        writer
            .write_all(
                format!(
                    "{} {} {} {} {} {}\n",
                    defender.hp,
                    defender.range,
                    defender.attack_power,
                    0,
                    defender.price,
                    0
                )
                .as_bytes(),
            )
            .unwrap();
    }

    writer
}

pub fn send_initial_pvp_input(fifos: Vec<&File>, pvp_request: &PvPGameRequest) {
    for fifo in fifos {
        let writer = BufWriter::new(fifo);
        let _ = send_initial_parameters(writer, &pvp_request.parameters);
    }
}

pub fn send_initial_input(fifos: Vec<&File>, normal_game_request: &NormalGameRequest) {
    for fifo in fifos {
        let writer = BufWriter::new(fifo);
        let mut writer = send_initial_parameters(writer, &normal_game_request.parameters);

        writer.write_all("64 64\n".as_bytes()).unwrap();

        for row in normal_game_request.map.iter() {
            for cell in row.iter() {
                writer.write_all(format!("{cell} ").as_bytes()).unwrap();
            }
            writer.write_all("\n".as_bytes()).unwrap();
        }
    }
}

pub fn make_copy(
    src_dir: &str,
    dest_dir: &str,
    player_code_file: &str,
    game_id: &String,
    player_code: &PlayerCode,
) -> Option<response::GameStatus> {
    if let Err(e) = copy_dir_all(src_dir, dest_dir) {
        return Some(create_error_response(
            game_id.clone(),
            error::SimulatorError::UnidentifiedError(format!(
                "Failed to copy player code boilerplate: {e}"
            )),
        ));
    }

    if let Err(e) = std::fs::File::create(player_code_file).and_then(|mut file| {
        file.write_all(player_code.source_code.as_bytes())
            .and_then(|_| file.sync_all())
    }) {
        return Some(create_error_response(
            game_id.to_owned(),
            error::SimulatorError::UnidentifiedError(format!("Failed to copy player code: {e}")),
        ));
    }
    None
}

pub fn copy_files(
    game_id: &String,
    player_code: &PlayerCode,
    game_dir_handle: &GameDir,
    player_dir: &String,
    game_type: &GameType,
) -> Option<GameStatus> {
    let (to_copy_dir, player_code_file) = match player_code.language {
        Language::CPP => (
            "player_code/cpp",
            format!(
                "{}/{}/{}.cpp",
                game_dir_handle.get_path(),
                player_dir,
                game_type.file_name(Language::CPP)
            ),
        ),
        Language::PYTHON => (
            "player_code/python",
            format!(
                "{}/{}/{}.py",
                game_dir_handle.get_path(),
                player_dir,
                game_type.file_name(Language::PYTHON)
            ),
        ),
        Language::JAVA => (
            "player_code/java",
            format!(
                "{}/{}/{}.java",
                game_dir_handle.get_path(),
                player_dir,
                game_type.file_name(Language::JAVA)
            ),
        ),
    };

    make_copy(
        to_copy_dir,
        format!("{}/{}", game_dir_handle.get_path(), player_dir).as_str(),
        &player_code_file,
        game_id,
        player_code,
    )
}