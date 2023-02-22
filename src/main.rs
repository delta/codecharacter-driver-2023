use std::{env, sync::Arc};

use cc_driver::{
    create_error_response, create_executing_response,
    error::SimulatorError,
    fifo::Fifo,
    game_dir::GameDir,
    mq::{consumer, Publisher},
    poll::{
        epoll::{CallbackMessage, EpollGeneric},
        epoll_entry::{EpollEntryType, Process, ProcessOutput, ProcessType},
    },
    request::{GameRequest, Language},
    response::GameStatus,
    runner::{cpp, java, py, simulator, Runnable},
};
use log::{info, LevelFilter};
use log4rs::{
    append::{
        console::{ConsoleAppender, Target},
        file::FileAppender,
    },
    config::{Appender, Config, Root},
    filter::threshold::ThresholdFilter,
};
use nix::sys::epoll::EpollFlags;

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

fn handler(game_request: GameRequest) -> GameStatus {
    info!(
        "Starting execution for {} with language {:?}",
        game_request.game_id, game_request.language
    );
    let game_dir_handle = GameDir::new(&game_request.game_id);

    if game_dir_handle.is_none() {
        return create_error_response(
            &game_request,
            cc_driver::error::SimulatorError::UnidentifiedError(
                "Failed to create game directory".to_owned(),
            ),
        );
    }

    let game_dir_handle = game_dir_handle.unwrap();

    let (to_copy_dir, player_code_file) = match game_request.language {
        cc_driver::request::Language::CPP => (
            "player_code/cpp",
            format!("{}/run.cpp", game_dir_handle.get_path()),
        ),
        cc_driver::request::Language::PYTHON => (
            "player_code/python",
            format!("{}/run.py", game_dir_handle.get_path()),
        ),
        cc_driver::request::Language::JAVA => (
            "player_code/java",
            format!("{}/Run.java", game_dir_handle.get_path()),
        ),
    };

    if let Some(resp) = cc_driver::utils::make_copy(
        to_copy_dir,
        game_dir_handle.get_path(),
        &player_code_file,
        &game_request,
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

            cc_driver::utils::send_initial_input(vec![&p1_stdout, &p2_stdout], &game_request);

            let runner: Box<dyn Runnable> = match game_request.language {
                Language::CPP => Box::new(cpp::Runner::new(
                    game_dir_handle.get_path().to_string(),
                    game_request.game_id.to_string(),
                )),
                Language::PYTHON => Box::new(py::Runner::new(
                    game_dir_handle.get_path().to_string(),
                    game_request.game_id.to_string(),
                )),
                Language::JAVA => Box::new(java::Runner::new(
                    game_dir_handle.get_path().to_string(),
                    game_request.game_id.to_string(),
                )),
            };

            let initialize = || -> Result<_, SimulatorError> {
                let mut player_process = runner.run(p1_stdin, p1_stdout)?;
                let simulator = simulator::Simulator::new(game_request.game_id.to_string());
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
                Err(err) => return create_error_response(&game_request, err),
            };

            let mut outputs: Vec<ProcessOutput> = vec![];

            while !event_handler.is_empty() {
                let result = handle_event(&mut event_handler);
                match result {
                    Ok(processing_outputs) => {
                        outputs.extend(processing_outputs.into_iter().flatten())
                    }
                    Err(err) => return create_error_response(&game_request, err),
                }
            }

            let process1 = outputs.remove(0);
            let process2 = outputs.remove(0);

            let (player_process_out, sim_process_out) = match process1.process_type() {
                ProcessType::Runner => (process1.output(), process2.output()),
                ProcessType::Simulator => (process2.output(), process1.output()),
            };

            info!("Successfully executed for game {}", game_request.game_id);
            cc_driver::create_final_response(game_request, player_process_out, sim_process_out)
        }

        (Err(e), _) | (_, Err(e)) => create_error_response(&game_request, e),
    }
}

fn worker_fn(msg_receiver: crossbeam_channel::Receiver<GameRequest>, publisher: Arc<Publisher>) {
    while let Ok(req) = msg_receiver.recv() {
        // publishing error means we can crash, something is wrong
        publisher.publish(create_executing_response(&req)).unwrap();
        let response = handler(req);
        publisher.publish(response).unwrap();
    }
}

fn main() {
    let level = log::LevelFilter::Info;
    let file_path = "driver.log";

    let stderr = ConsoleAppender::builder().target(Target::Stderr).build();

    let logfile = FileAppender::builder().build(file_path).unwrap();

    let config = Config::builder()
        .appender(Appender::builder().build("logfile", Box::new(logfile)))
        .appender(
            Appender::builder()
                .filter(Box::new(ThresholdFilter::new(level)))
                .build("stderr", Box::new(stderr)),
        )
        .build(
            Root::builder()
                .appender("logfile")
                .appender("stderr")
                .build(LevelFilter::Info),
        )
        .unwrap();

    let _handle = log4rs::init_config(config).unwrap();

    let res = consumer(
        env::var("RABBITMQ_HOST").unwrap(),
        env::var("REQUEST_QUEUE").unwrap(),
        env::var("RESPONSE_QUEUE").unwrap(),
        worker_fn,
    );

    match res {
        Ok(_) => {}
        Err(e) => {
            println!("{e}");
        }
    }
}
