use std::env;

use log::info;
use nix::sys::epoll::EpollFlags;

use crate::{
    create_final_pvp_response, create_final_response, create_normal_error_response,
    create_pvp_error_response,
    error::SimulatorError,
    fifo::Fifo,
    game_dir::GameDir,
    poll::{
        epoll::{CallbackMessage, EpollGeneric},
        epoll_entry::{EpollEntryType, Process, ProcessOutput, ProcessType},
    },
    request::{GameRequest, Language, NormalGameRequest, PlayerCode, PvPGameRequest},
    response::GameStatus,
    runner::{cpp, java, py, simulator, GameType, Runnable},
    utils::{copy_files, send_initial_input, send_initial_pvp_input},
};

pub trait Handler {
    fn handle(self) -> GameStatus;
}

fn handle_event(
    epoll_handle: &mut EpollGeneric<EpollEntryType>,
) -> Result<(Vec<Option<ProcessOutput>>, Vec<Option<ProcessType>>), SimulatorError> {
    let events = epoll_handle.poll(
        env::var("EPOLL_WAIT_TIMEOUT")
            .unwrap()
            .parse()
            .unwrap_or(1000),
        epoll_handle.get_registered_fds().len(),
    )?;
    let mut res = vec![];
    let mut errors = vec![];
    for e in events {
        match epoll_handle.process_event(e)? {
            CallbackMessage::Unregister(fd) => {
                // Means it's a stderr handle
                let entry = epoll_handle.unregister(fd as u64)?;
                res.push(match entry {
                    EpollEntryType::Process(_) => unreachable!(),
                    EpollEntryType::StdErr(output) => Some(output),
                });
            }
            CallbackMessage::HandleExplicitly(fd) => {
                // Means its a process handle
                let entry = epoll_handle.unregister(fd as u64)?;
                match entry {
                    EpollEntryType::StdErr(_) => unreachable!(),
                    EpollEntryType::Process(mut p) => {
                        let exit_status = p.wait()?;
                        if exit_status.success() {
                            res.push(None);
                        } else {
                            let killable_processes = epoll_handle
                                .get_registered_fds()
                                .iter()
                                .filter_map(|x| match x.1 {
                                    EpollEntryType::Process(_) => Some(*x.0),
                                    _ => None,
                                })
                                .collect::<Vec<u64>>();
                            killable_processes.iter().for_each(|x| {
                                match epoll_handle.unregister(*x).unwrap() {
                                    EpollEntryType::Process(mut p) => p.kill(),
                                    EpollEntryType::StdErr(_) => unreachable!(),
                                }
                            });
                            match p.get_type() {
                                &ProcessType::Runner => errors.push(Some(ProcessType::Runner)),
                                &ProcessType::RunnerPlayer1 => {
                                    errors.push(Some(ProcessType::RunnerPlayer1))
                                }
                                &ProcessType::RunnerPlayer2 => {
                                    errors.push(Some(ProcessType::RunnerPlayer2))
                                }
                                &ProcessType::Simulator => {
                                    errors.push(Some(ProcessType::Simulator))
                                }
                            }
                        }
                    }
                }
            }
            CallbackMessage::Nop => {
                res.push(None);
            }
        }
    }
    Ok((res, errors))
}

fn get_runner(
    player_code: &PlayerCode,
    game_id: &String,
    game_dir_handle: &GameDir,
    player_dir: &String,
) -> Box<dyn Runnable> {
    match player_code.language {
        Language::CPP => Box::new(cpp::Runner::new(
            game_dir_handle.get_path().to_string(),
            game_id.to_string(),
            player_dir.to_owned(),
        )),
        Language::PYTHON => Box::new(py::Runner::new(
            game_dir_handle.get_path().to_string(),
            game_id.to_string(),
            player_dir.to_owned(),
        )),
        Language::JAVA => Box::new(java::Runner::new(
            game_dir_handle.get_path().to_string(),
            game_id.to_string(),
            player_dir.to_owned(),
        )),
    }
}

impl Handler for NormalGameRequest {
    fn handle(self) -> GameStatus {
        info!(
            "Starting normal game execution for {} with language {:?}",
            self.game_id, self.player_code.language
        );
        let game_dir_handle = GameDir::new(&self.game_id);

        if game_dir_handle.is_none() {
            return create_normal_error_response(
                self.game_id,
                SimulatorError::UnidentifiedError("Failed to create game directory".to_owned()),
            );
        }

        let game_dir_handle = game_dir_handle.unwrap();
        let player_dir = "player".to_string();
        game_dir_handle.create_sub_dir(&player_dir);

        if let Some(resp) = copy_files(
            &self.game_id,
            &self.player_code,
            &game_dir_handle,
            &player_dir,
            &GameType::NormalGame,
        ) {
            return resp;
        }

        let p1_in = format!("{}/p1_in", game_dir_handle.get_path());
        let p2_in = format!("{}/p2_in", game_dir_handle.get_path());

        let pipe1 = Fifo::new(p1_in);
        let pipe2 = Fifo::new(p2_in);

        match (pipe1, pipe2) {
            (Ok(mut p1), Ok(mut p2)) => {
                let (p1_stdin, p2_stdout) = p1.get_ends().unwrap();
                let (p2_stdin, p1_stdout) = p2.get_ends().unwrap();

                send_initial_input(vec![&p1_stdout, &p2_stdout], &self);

                let runner = get_runner(
                    &self.player_code,
                    &self.game_id,
                    &game_dir_handle,
                    &player_dir,
                );

                let initialize = || -> Result<_, SimulatorError> {
                    let mut player_process =
                        runner.run(p1_stdin, p1_stdout, GameType::NormalGame)?;
                    let simulator = simulator::Simulator::new(self.game_id.to_string());
                    let mut sim_process = simulator.run(p2_stdin, p2_stdout)?;

                    let player_stderr = player_process.stderr.take().unwrap();
                    let sim_stderr = sim_process.stderr.take().unwrap();

                    let player_process = Process::new(player_process, ProcessType::Runner);
                    let sim_process = Process::new(sim_process, ProcessType::Simulator);
                    let player_output = ProcessOutput::new(player_stderr, ProcessType::Runner);
                    let sim_output = ProcessOutput::new(sim_stderr, ProcessType::Simulator);

                    let player = EpollEntryType::Process(player_process);
                    let player_output = EpollEntryType::StdErr(player_output);
                    let sim = EpollEntryType::Process(sim_process);
                    let sim_output = EpollEntryType::StdErr(sim_output);

                    let mut event_handler =
                        EpollGeneric::<EpollEntryType>::new().map_err(SimulatorError::from)?;

                    event_handler
                        .register(player, EpollFlags::EPOLLIN | EpollFlags::EPOLLHUP)
                        .map_err(SimulatorError::from)?;
                    event_handler
                        .register(player_output, EpollFlags::EPOLLIN | EpollFlags::EPOLLHUP)
                        .map_err(SimulatorError::from)?;
                    event_handler
                        .register(sim, EpollFlags::EPOLLIN | EpollFlags::EPOLLHUP)
                        .map_err(SimulatorError::from)?;
                    event_handler
                        .register(sim_output, EpollFlags::EPOLLIN | EpollFlags::EPOLLHUP)
                        .map_err(SimulatorError::from)?;

                    Ok(event_handler)
                };

                let mut event_handler = match initialize() {
                    Ok(handler) => handler,
                    Err(err) => return create_normal_error_response(self.game_id, err),
                };

                let mut outputs: Vec<ProcessOutput> = vec![];

                while !event_handler.is_empty() {
                    match handle_event(&mut event_handler) {
                        Ok((result, errors)) => {
                            outputs.extend(result.into_iter().flatten());
                            if !errors.is_empty() {
                                while !outputs.is_empty() {
                                    let process = outputs.remove(0);
                                    match process.process_type() {
                                        ProcessType::Runner => {
                                            return create_normal_error_response(
                                                self.game_id,
                                                SimulatorError::RuntimeError(process.output()),
                                            )
                                        }
                                        _ => {
                                            return create_normal_error_response(self.game_id, SimulatorError::RuntimeError("couldnt communicate with simulator check syntax".to_owned()));
                                        }
                                    }
                                }
                            }
                        }
                        Err(_) => {
                            return create_normal_error_response(
                                self.game_id,
                                SimulatorError::RuntimeError("Unknown error occured".to_owned()),
                            );
                        }
                    }
                }

                let process1 = outputs.remove(0);
                let process2 = outputs.remove(0);

                let (player_process_out, sim_process_out) = match process1.process_type() {
                    ProcessType::Runner => (process1.output(), process2.output()),
                    ProcessType::Simulator => (process2.output(), process1.output()),
                    _ => {
                        return create_normal_error_response(
                            self.game_id.to_owned(),
                            SimulatorError::UnidentifiedError("Failed to map outputs".to_owned()),
                        );
                    }
                };

                info!("Successfully executed for game {}", self.game_id);
                create_final_response(
                    self.parameters,
                    self.game_id,
                    player_process_out,
                    sim_process_out,
                )
            }

            (Err(e), _) | (_, Err(e)) => create_normal_error_response(self.game_id, e),
        }
    }
}

impl Handler for PvPGameRequest {
    fn handle(self) -> GameStatus {
        info!(
            "Starting pvp game execution for {} with languages player1: {:?} and player2: {:?}",
            self.game_id, self.player1.language, self.player2.language
        );
        let game_dir_handle = GameDir::new(&self.game_id);

        if game_dir_handle.is_none() {
            return create_pvp_error_response(
                self.game_id,
                SimulatorError::UnidentifiedError("Failed to create game directory".to_owned()),
                SimulatorError::UnidentifiedError("Failed to create game directory".to_owned()),
                true,
                true,
            );
        }

        let game_dir_handle = game_dir_handle.unwrap();
        let player1_dir = "pvp_game/player_1";
        let player2_dir = "pvp_game/player_2";

        game_dir_handle.create_sub_dir(player1_dir);
        game_dir_handle.create_sub_dir(player2_dir);

        //print all files in game_dir_handle

        if let Some(resp) = copy_files(
            &self.game_id,
            &self.player1,
            &game_dir_handle,
            &player1_dir.to_string(),
            &GameType::PvPGame,
        ) {
            return resp;
        }

        if let Some(resp) = copy_files(
            &self.game_id,
            &self.player2,
            &game_dir_handle,
            &player2_dir.to_string(),
            &GameType::PvPGame,
        ) {
            return resp;
        }

        let p1_in = format!("{}/p1_in", game_dir_handle.get_path());
        let p2_in = format!("{}/p2_in", game_dir_handle.get_path());
        let p3_in = format!("{}/p3_in", game_dir_handle.get_path());
        let p4_in = format!("{}/p4_in", game_dir_handle.get_path());
        let p5_in = format!("{}/p5_in", game_dir_handle.get_path());

        let pipe1 = Fifo::new(p1_in.to_owned());
        let pipe2 = Fifo::new(p2_in.to_owned());
        let pipe3 = Fifo::new(p3_in.to_owned());
        let pipe4 = Fifo::new(p4_in.to_owned());
        let pipe5 = Fifo::new(p5_in.to_owned());

        match (pipe1, pipe2, pipe3, pipe4, pipe5) {
            (Ok(mut p1), Ok(mut p2), Ok(mut p3), Ok(mut p4), Ok(mut p5)) => {
                let (_sim_p1_r, p1_w) = p1.get_ends().unwrap();
                let (_sim_p2_r, p2_w) = p2.get_ends().unwrap();
                let (p1_r, sim_p1_w) = p3.get_ends().unwrap();
                let (p2_r, sim_p2_w) = p4.get_ends().unwrap();
                let (sim_r, sim_w) = p5.get_ends().unwrap();

                send_initial_pvp_input(vec![&sim_p1_w, &sim_p2_w, &sim_w], &self);

                let runner1 = get_runner(
                    &self.player1,
                    &self.game_id,
                    &game_dir_handle,
                    &player1_dir.to_string(),
                );
                let runner2 = get_runner(
                    &self.player2,
                    &self.game_id,
                    &game_dir_handle,
                    &player2_dir.to_string(),
                );
                let initialize = || -> Result<_, SimulatorError> {
                    let mut player1_process =
                        runner1
                            .run(p1_r, p1_w, GameType::PvPGame)
                            .map_err(|e| match e {
                                SimulatorError::CompilationError(e) => {
                                    SimulatorError::Player1Error(e)
                                }
                                _ => SimulatorError::Player1Error("Couldnt compile".to_owned()),
                            })?;
                    let mut player2_process =
                        runner2
                            .run(p2_r, p2_w, GameType::PvPGame)
                            .map_err(|e| match e {
                                SimulatorError::CompilationError(e) => {
                                    SimulatorError::Player2Error(e)
                                }
                                _ => SimulatorError::Player2Error("Couldnt compile".to_owned()),
                            })?;
                    let simulator = simulator::Simulator::new(self.game_id.to_string());
                    let mut sim_process =
                        simulator.run_pvp(sim_r, sim_w, p1_in, p3_in, p2_in, p4_in)?;

                    let player1_stderr = player1_process.stderr.take().unwrap();
                    let player2_stderr = player2_process.stderr.take().unwrap();

                    let sim_stderr = sim_process.stderr.take().unwrap();

                    let player1_process = Process::new(player1_process, ProcessType::RunnerPlayer1);
                    let player2_process = Process::new(player2_process, ProcessType::RunnerPlayer2);
                    let sim_process = Process::new(sim_process, ProcessType::Simulator);

                    let player1_output =
                        ProcessOutput::new(player1_stderr, ProcessType::RunnerPlayer1);
                    let player2_output =
                        ProcessOutput::new(player2_stderr, ProcessType::RunnerPlayer2);
                    let sim_output = ProcessOutput::new(sim_stderr, ProcessType::Simulator);

                    let player1 = EpollEntryType::Process(player1_process);
                    let player2 = EpollEntryType::Process(player2_process);

                    let player1_output = EpollEntryType::StdErr(player1_output);
                    let player2_output = EpollEntryType::StdErr(player2_output);
                    let sim = EpollEntryType::Process(sim_process);
                    let sim_output = EpollEntryType::StdErr(sim_output);

                    let mut event_handler =
                        EpollGeneric::<EpollEntryType>::new().map_err(SimulatorError::from)?;

                    event_handler
                        .register(player1, EpollFlags::EPOLLIN | EpollFlags::EPOLLHUP)
                        .map_err(SimulatorError::from)?;
                    event_handler
                        .register(player1_output, EpollFlags::EPOLLIN | EpollFlags::EPOLLHUP)
                        .map_err(SimulatorError::from)?;
                    event_handler
                        .register(player2, EpollFlags::EPOLLIN | EpollFlags::EPOLLHUP)
                        .map_err(SimulatorError::from)?;
                    event_handler
                        .register(player2_output, EpollFlags::EPOLLIN | EpollFlags::EPOLLHUP)
                        .map_err(SimulatorError::from)?;
                    event_handler
                        .register(sim, EpollFlags::EPOLLIN | EpollFlags::EPOLLHUP)
                        .map_err(SimulatorError::from)?;
                    event_handler
                        .register(sim_output, EpollFlags::EPOLLIN | EpollFlags::EPOLLHUP)
                        .map_err(SimulatorError::from)?;
                    Ok(event_handler)
                };

                let mut event_handler = match initialize() {
                    Ok(handler) => handler,
                    Err(err) => match err {
                        SimulatorError::Player1Error(error) => {
                            return create_pvp_error_response(
                                self.game_id,
                                SimulatorError::CompilationError(format!("{error}")),
                                SimulatorError::CompilationError(
                                    "Other player couldnt compile".to_owned(),
                                ),
                                true,
                                false,
                            )
                        }
                        SimulatorError::Player2Error(error) => {
                            return create_pvp_error_response(
                                self.game_id,
                                SimulatorError::CompilationError(
                                    "Other player couldnt compile".to_owned(),
                                ),
                                SimulatorError::CompilationError(format!("{error}")),
                                false,
                                true,
                            )
                        }
                        _ => {
                            return create_pvp_error_response(
                                self.game_id.to_owned(),
                                err.clone(),
                                err,
                                true,
                                true,
                            )
                        }
                    },
                };

                let mut outputs: Vec<ProcessOutput> = vec![];
                let mut all_errors: Vec<ProcessType> = vec![];

                while !event_handler.is_empty() {
                    match handle_event(&mut event_handler) {
                        Ok((result, errors)) => {
                            outputs.extend(result.into_iter().flatten());
                            all_errors.extend(errors.into_iter().flatten());
                        }
                        Err(_) => {
                            // Handle the case where handle_event returns an error
                            // Adjust the error message as needed
                            return create_pvp_error_response(
                                self.game_id,
                                SimulatorError::RuntimeError("Unknown runtime error".to_owned()),
                                SimulatorError::RuntimeError("Unknown runtime error".to_owned()),
                                true,
                                true,
                            );
                        }
                    }
                }

                if all_errors.contains(&ProcessType::RunnerPlayer1) {
                    return create_pvp_error_response(
                        self.game_id,
                        SimulatorError::RuntimeError(
                            outputs
                                .into_iter()
                                .filter(|x| x.process_type() == &ProcessType::RunnerPlayer1)
                                .map(|x| x.output())
                                .collect::<String>(),
                        ),
                        SimulatorError::RuntimeError("the other player threw an error".to_owned()),
                        true,
                        false,
                    );
                }

                if all_errors.contains(&ProcessType::RunnerPlayer2) {
                    return create_pvp_error_response(
                        self.game_id,
                        SimulatorError::RuntimeError("the other player threw an error".to_owned()),
                        SimulatorError::RuntimeError(
                            outputs
                                .into_iter()
                                .filter(|x| x.process_type() == &ProcessType::RunnerPlayer2)
                                .map(|x| x.output())
                                .collect::<String>(),
                        ),
                        false,
                        true,
                    );
                }

                let process1 = outputs.remove(0);
                let process2 = outputs.remove(0);
                let process3 = outputs.remove(0);

                let (player1_process_out, player2_process_out, sim_process_out) = match (
                    process1.process_type(),
                    process2.process_type(),
                    process3.process_type(),
                ) {
                    (
                        ProcessType::RunnerPlayer1,
                        ProcessType::RunnerPlayer2,
                        ProcessType::Simulator,
                    ) => (process1.output(), process2.output(), process3.output()),
                    (
                        ProcessType::RunnerPlayer2,
                        ProcessType::RunnerPlayer1,
                        ProcessType::Simulator,
                    ) => (process2.output(), process1.output(), process3.output()),
                    (
                        ProcessType::RunnerPlayer1,
                        ProcessType::Simulator,
                        ProcessType::RunnerPlayer2,
                    ) => (process1.output(), process3.output(), process2.output()),
                    (
                        ProcessType::RunnerPlayer2,
                        ProcessType::Simulator,
                        ProcessType::RunnerPlayer1,
                    ) => (process3.output(), process1.output(), process2.output()),
                    (
                        ProcessType::Simulator,
                        ProcessType::RunnerPlayer1,
                        ProcessType::RunnerPlayer2,
                    ) => (process2.output(), process3.output(), process1.output()),
                    (
                        ProcessType::Simulator,
                        ProcessType::RunnerPlayer2,
                        ProcessType::RunnerPlayer1,
                    ) => (process3.output(), process2.output(), process1.output()),
                    _ => {
                        return create_pvp_error_response(
                            self.game_id.to_owned(),
                            SimulatorError::UnidentifiedError("Failed to map outputs".to_owned()),
                            SimulatorError::UnidentifiedError("Failed to map outputs".to_owned()),
                            true,
                            true,
                        );
                    }
                };

                info!("Successfully executed for game {}", self.game_id);
                create_final_pvp_response(
                    self.game_id,
                    player1_process_out,
                    player2_process_out,
                    sim_process_out,
                )
            }

            (Err(e), _, _, _, _)
            | (_, Err(e), _, _, _)
            | (_, _, Err(e), _, _)
            | (_, _, _, Err(e), _)
            | (_, _, _, _, Err(e)) => {
                create_pvp_error_response(self.game_id, e.clone(), e, true, true)
            }
        }
    }
}

impl Handler for GameRequest {
    fn handle(self) -> GameStatus {
        match self {
            GameRequest::NormalGame(request) => request.handle(),
            GameRequest::PvPGame(request) => request.handle(),
        }
    }
}
