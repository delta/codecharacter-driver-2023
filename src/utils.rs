use std::{
    env,
    fs::File,
    io::{BufWriter, Write},
};

use fs_extra::dir::CopyOptions;

use crate::{
    create_error_response, error,
    game_dir::GameDir,
    request::{Attacker, Defender, Language, NormalGameRequest, PlayerCode, PvPGameRequest},
    response::{self, GameStatus},
    runner::GameType,
};

pub fn copy_dir_all(
    src: impl AsRef<std::path::Path>,
    dst: impl AsRef<std::path::Path>,
) -> std::io::Result<()> {
    let opt = CopyOptions::new();
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

pub fn send_troops<'a>(
    mut writer: BufWriter<&'a File>,
    attackers: &Vec<Attacker>,
    defenders: &Vec<Defender>,
) -> BufWriter<&'a File> {
    writer
        .write_all(format!("{}\n", attackers.len()).as_bytes())
        .unwrap();
    for attacker in attackers {
        writer
            .write_all(
                format!(
                    "{} {} {} {} {} {} {}\n",
                    attacker.hp,
                    attacker.range,
                    attacker.attack_power,
                    attacker.speed,
                    attacker.price,
                    attacker.is_aerial,
                    attacker.weight
                )
                .as_bytes(),
            )
            .unwrap();
    }
    writer
        .write_all(format!("{}\n", defenders.len()).as_bytes())
        .unwrap();
    for defender in defenders {
        writer
            .write_all(
                format!(
                    "{} {} {} {} {} {}\n",
                    defender.hp,
                    defender.range,
                    defender.attack_power,
                    0,
                    defender.price,
                    defender.is_aerial
                )
                .as_bytes(),
            )
            .unwrap();
    }

    writer
}

pub fn send_initial_pvp_input(fifos: Vec<&File>, pvp_request: &PvPGameRequest) {
    for fifo in fifos {
        let mut writer = BufWriter::new(fifo);
        writer
            .write_all(
                format!(
                    "{} {}\n",
                    pvp_request.parameters.no_of_turns, pvp_request.parameters.no_of_coins
                )
                .as_bytes(),
            )
            .unwrap();
        let _ = send_troops(
            writer,
            &pvp_request.parameters.attackers,
            &pvp_request.parameters.defenders,
        );
    }
}

pub fn send_initial_input(fifos: Vec<&File>, normal_game_request: &NormalGameRequest) {
    for fifo in fifos {
        let mut writer = BufWriter::new(fifo);
        writer
            .write_all(
                format!(
                    "{} {}\n",
                    normal_game_request.parameters.no_of_turns,
                    normal_game_request.parameters.no_of_coins
                )
                .as_bytes(),
            )
            .unwrap();
        let mut writer = send_troops(
            writer,
            &normal_game_request.parameters.attackers,
            &normal_game_request.parameters.defenders,
        );
        writer
            .write_all(
                format!(
                    "{} {}\n",
                    env::var("MAP_SIZE").unwrap(),
                    env::var("MAP_SIZE").unwrap()
                )
                .as_bytes(),
            )
            .unwrap();

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
