use std::{env, os::fd::AsRawFd};

use log::info;
use nix::sys::epoll::EpollFlags;

use crate::{
    create_error_response, create_final_response,
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
    utils::{make_copy, send_initial_input, send_initial_pvp_input},
};

pub trait Handler {
    fn handle(self) -> GameStatus;
}

fn handle_event(
    epoll_handle: &mut EpollGeneric<EpollEntryType>,
) -> Result<Vec<Option<ProcessOutput>>, SimulatorError> {
    let events = epoll_handle.poll(
        env::var("EPOLL_WAIT_TIMEOUT").unwrap().parse().unwrap(),
        epoll_handle.get_registered_fds().len(),
    )?;
    let mut res = vec![];
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

                            return Err(match exit_status.code() {
                            // 137 => Stands for container killing itself (by SIGKILL)
                            // that will be due to contraint provided
                            None | Some(137) => SimulatorError::TimeOutError(
                                "Process took longer than the specified time to execute, so it was killed"
                                    .to_owned(),
                            ),
                            Some(code) => SimulatorError::RuntimeError(format!(
                                "Program exited with non zero exit code: {code}"
                            )),
                            });
                        }
                    }
                }
            }
            CallbackMessage::Nop => {
                res.push(None);
            }
        }
    }
    Ok(res)
}

fn get_runner(
    player_code: &PlayerCode,
    game_id: &String,
    game_dir_handle: &GameDir,
    file_name: &String,
) -> Box<dyn Runnable> {
    match player_code.language {
        Language::CPP => Box::new(cpp::Runner::new(
            game_dir_handle.get_path().to_string(),
            game_id.to_string(),
            file_name.to_owned(),
        )),
        Language::PYTHON => Box::new(py::Runner::new(
            game_dir_handle.get_path().to_string(),
            game_id.to_string(),
            file_name.to_owned(),
        )),
        Language::JAVA => Box::new(java::Runner::new(
            game_dir_handle.get_path().to_string(),
            game_id.to_string(),
            file_name.to_owned(),
        )),
    }
}

fn copy_files(
    game_id: &String,
    player_code: &PlayerCode,
    game_dir_handle: &GameDir,
    file_name: &String,
) -> Option<GameStatus> {
    let (to_copy_dir, player_code_file) = match player_code.language {
        Language::CPP => (
            "player_code/cpp",
            format!("{}/{}.cpp", game_dir_handle.get_path(), file_name),
        ),
        Language::PYTHON => (
            "player_code/python",
            format!("{}/{}.py", game_dir_handle.get_path(), file_name),
        ),
        Language::JAVA => (
            "player_code/java",
            format!("{}/{}.java", game_dir_handle.get_path(), file_name),
        ),
    };

    make_copy(
        to_copy_dir,
        game_dir_handle.get_path(),
        &player_code_file,
        game_id,
        player_code,
    )
}

impl Handler for NormalGameRequest {
    fn handle(self) -> GameStatus {
        info!(
            "Starting normal game execution for {} with language {:?}",
            self.game_id, self.player_code.language
        );
        let game_dir_handle = GameDir::new(&self.game_id);

        if game_dir_handle.is_none() {
            return create_error_response(
                self.game_id,
                SimulatorError::UnidentifiedError("Failed to create game directory".to_owned()),
            );
        }

        let game_dir_handle = game_dir_handle.unwrap();
        let file_name = "run".to_string();

        if let Some(resp) = copy_files(
            &self.game_id,
            &self.player_code,
            &game_dir_handle,
            &file_name,
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
                    &file_name,
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
                    Err(err) => return create_error_response(self.game_id, err),
                };

                let mut outputs: Vec<ProcessOutput> = vec![];

                while !event_handler.is_empty() {
                    let result = handle_event(&mut event_handler);
                    match result {
                        Ok(processing_outputs) => {
                            outputs.extend(processing_outputs.into_iter().flatten())
                        }
                        Err(err) => return create_error_response(self.game_id, err),
                    }
                }

                let process1 = outputs.remove(0);
                let process2 = outputs.remove(0);

                let (player_process_out, sim_process_out) = match process1.process_type() {
                    ProcessType::Runner => (process1.output(), process2.output()),
                    ProcessType::Simulator => (process2.output(), process1.output()),
                };

                info!("Successfully executed for game {}", self.game_id);
                create_final_response(
                    self.parameters,
                    self.game_id,
                    player_process_out,
                    sim_process_out,
                )
            }

            (Err(e), _) | (_, Err(e)) => create_error_response(self.game_id, e),
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
            return create_error_response(
                self.game_id,
                SimulatorError::UnidentifiedError("Failed to create game directory".to_owned()),
            );
        }

        let game_dir_handle = game_dir_handle.unwrap();
        let player1_file_name = "player1".to_string();
        let player2_file_name = "player2".to_string();

        // to_copy_dir = "player_code/cpp"
        // player_code_file = "/etc/{game_id}/run.cpp"

        if let Some(resp) = copy_files(
            &self.game_id,
            &self.player1,
            &game_dir_handle,
            &player1_file_name,
        ) {
            return resp;
        }

        if let Some(resp) = copy_files(
            &self.game_id,
            &self.player2,
            &game_dir_handle,
            &player2_file_name,
        ) {
            return resp;
        }

        let p1_in = format!("{}/p1_in", game_dir_handle.get_path());
        let p2_in = format!("{}/p2_in", game_dir_handle.get_path());
        let p3_in = format!("{}/p3_in", game_dir_handle.get_path());
        let p4_in = format!("{}/p4_in", game_dir_handle.get_path());
        let p5_in = format!("{}/p5_in", game_dir_handle.get_path());

        let pipe1 = Fifo::new(p1_in);
        let pipe2 = Fifo::new(p2_in);
        let pipe3 = Fifo::new(p3_in);
        let pipe4 = Fifo::new(p4_in);
        let pipe5 = Fifo::new(p5_in);

        match (pipe1, pipe2, pipe3, pipe4, pipe5) {
            (Ok(mut p1), Ok(mut p2), Ok(mut p3), Ok(mut p4), Ok(mut p5)) => {
                let (sim_p1_r, p1_w) = p1.get_ends().unwrap();
                let (sim_p2_r, p2_w) = p2.get_ends().unwrap();
                let (p1_r, sim_p1_w) = p3.get_ends().unwrap();
                let (p2_r, sim_p2_w) = p4.get_ends().unwrap();
                let (sim_r, sim_w) = p5.get_ends().unwrap();

                send_initial_pvp_input(vec![&p1_w, &p2_w, &sim_w], &self);

                let runner1 = get_runner(
                    &self.player1,
                    &self.game_id,
                    &game_dir_handle,
                    &player1_file_name,
                );
                let runner2 = get_runner(
                    &self.player2,
                    &self.game_id,
                    &game_dir_handle,
                    &player2_file_name,
                );

                let initialize = || -> Result<_, SimulatorError> {
                    let mut player1_process = runner1.run(p1_r, p1_w, GameType::PvPGame)?;
                    let mut player2_process = runner2.run(p2_r, p2_w, GameType::PvPGame)?;
                    let simulator = simulator::Simulator::new(self.game_id.to_string());

                    let mut sim_process = simulator.run_pvp(
                        sim_r,
                        sim_w,
                        sim_p1_r.as_raw_fd(),
                        sim_p1_w.as_raw_fd(),
                        sim_p2_r.as_raw_fd(),
                        sim_p2_w.as_raw_fd(),
                    )?;

                    let player1_stderr = player1_process.stderr.take().unwrap();
                    let player2_stderr = player2_process.stderr.take().unwrap();

                    let sim_stderr = sim_process.stderr.take().unwrap();

                    let player1_process = Process::new(player1_process, ProcessType::Runner);
                    let player2_process = Process::new(player2_process, ProcessType::Runner);
                    let sim_process = Process::new(sim_process, ProcessType::Simulator);

                    let player1_output = ProcessOutput::new(player1_stderr, ProcessType::Runner);
                    let player2_output = ProcessOutput::new(player2_stderr, ProcessType::Runner);
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
                    Err(err) => return create_error_response(self.game_id.to_owned(), err),
                };

                let mut outputs: Vec<ProcessOutput> = vec![];

                while !event_handler.is_empty() {
                    let result = handle_event(&mut event_handler);
                    match result {
                        Ok(processing_outputs) => {
                            outputs.extend(processing_outputs.into_iter().flatten())
                        }
                        Err(err) => return create_error_response(self.game_id.to_owned(), err),
                    }
                }

                let process1 = outputs.remove(0);
                let process2 = outputs.remove(0);

                let (player_process_out, sim_process_out) = match process1.process_type() {
                    ProcessType::Runner => (process1.output(), process2.output()),
                    ProcessType::Simulator => (process2.output(), process1.output()),
                };

                info!("Successfully executed for game {}", self.game_id);
                create_final_response(
                    self.parameters,
                    self.game_id,
                    player_process_out,
                    sim_process_out,
                )
            }

            (Err(e), _, _, _, _)
            | (_, Err(e), _, _, _)
            | (_, _, Err(e), _, _)
            | (_, _, _, Err(e), _)
            | (_, _, _, _, Err(e)) => create_error_response(self.game_id, e),
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
