use std::sync::Arc;

use cc_driver::{
    runner::{cpp, java, py, simulator, Executable, Run},
    create_error_response, create_executing_response,
    error::SimulatorError,
    fifo::Fifo,
    game_dir::GameDir,
    mq::{consumer, Publisher},
    request::{GameRequest, Language},
    response::GameStatus, EPOLL_WAIT_TIMEOUT, poll::{epoll::Epoll, process::{ProcessOutput, Files, Process, ProcessType}},
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

fn handle_event(event_handler: &mut Epoll) -> Result<Option<ProcessOutput>, SimulatorError> {
    let (event, file) = match event_handler
        .poll(EPOLL_WAIT_TIMEOUT)
        .map_err(SimulatorError::from)? {
        Some(res) => res,
        None => return Ok(None)
    };

    match file {
        Files::Process(_) => {
            let fd = event.data();
            
            let mut proc = event_handler
                .unregister(fd)
                .map_err(SimulatorError::from)?
                .0.unwrap();

            let exit_status = match proc.wait() {
                Ok(status) => status,
                Err(err) => return Err(err)
            };

            if exit_status.success() {
                return Ok(None);
            }

            event_handler
                .clear_processes()
                .iter_mut()
                .for_each(|p| p.kill());

            Err(match exit_status.code() {
                // 137 => Stands for container killing itself (by SIGKILL) that will be due to contraint provided
                None | Some(137) => SimulatorError::TimeOutError("Process took longer than the specified time to execute, so it was killed".to_owned()),
                Some(code) => SimulatorError::RuntimeError(format!("Program exited with non zero exit code: {code}")),
            })
        },
        Files::StdErr(output) => {
            match event.events() {
                EpollFlags::EPOLLHUP => {
                    let output= event_handler
                        .unregister(event.data())
                        .map_err(SimulatorError::from)?
                        .1.unwrap();

                    Ok(Some(output))
                }
                _ => {
                    output.read_to_string()?;
                    Ok(None)
                }
            }
        }
    }
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

            let runner: Box<dyn Executable> = match game_request.language {
                Language::CPP => Box::new(
                    cpp::Runner::new(
                        game_dir_handle.get_path().to_string(),
                        game_request.game_id.to_string())),
                Language::PYTHON => Box::new(
                    py::Runner::new(
                        game_dir_handle.get_path().to_string(),
                        game_request.game_id.to_string())),
                Language::JAVA => Box::new(
                    java::Runner::new(
                        game_dir_handle.get_path().to_string(),
                        game_request.game_id.to_string())),
            };

            let intialize = || -> Result<Epoll, SimulatorError> {
                let mut player_process = runner.run(p1_stdin, p1_stdout)?;
                let simulator = simulator::Simulator::new(game_request.game_id.to_string());
                let mut sim_process = simulator.run(p2_stdin, p2_stdout)?;

                let player_stderr = player_process.stderr.take().unwrap();
                let sim_stderr = sim_process.stderr.take().unwrap();

                let player_process = Process::new(player_process, ProcessType::Runner);
                let sim_process = Process::new(sim_process, ProcessType::Simulator);
                let player_output = ProcessOutput::new(player_stderr, ProcessType::Runner);
                let sim_output = ProcessOutput::new(sim_stderr, ProcessType::Simulator);

                let player = Files::Process(player_process);
                let player_output = Files::StdErr(player_output);
                let sim = Files::Process(sim_process);
                let sim_output = Files::StdErr(sim_output);

                let mut event_handler: Epoll = Epoll::new()
                    .map_err(SimulatorError::from)?;

                event_handler.register(player).map_err(SimulatorError::from)?;
                event_handler.register(player_output).map_err(SimulatorError::from)?;
                event_handler.register(sim).map_err(SimulatorError::from)?;
                event_handler.register(sim_output).map_err(SimulatorError::from)?;

                Ok(event_handler)
            };

            let mut event_handler = match intialize() {
                Ok(handler) => handler,
                Err(err) => return create_error_response(&game_request, err),
            };

            let mut outputs = vec![];

            while !event_handler.is_empty() {
                let result = handle_event(&mut event_handler);

                match result {
                    Ok(Some(process_output)) => outputs.push(process_output),
                    Err(err) => return create_error_response(&game_request, err),
                    _ => {}
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

        (Err(e), _) | (_, Err(e)) => {
            create_error_response(&game_request, e)
        }
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
        "amqp://guest:guest@localhost".to_owned(),
        "gameRequestQueue".to_owned(),
        "gameStatusUpdateQueue".to_owned(),
        worker_fn,
    );

    match res {
        Ok(_) => {}
        Err(e) => {
            println!("{e}");
        }
    }
}
