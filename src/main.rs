use std::sync::Arc;

use cc_driver::{
    runner::{cpp, java, py, simulator, Executable},
    create_error_response, create_executing_response,
    error::SimulatorError,
    fifo::Fifo,
    game_dir::GameDir,
    mq::{consumer, Publisher},
    request::{GameRequest, Language},
    response::GameStatus,
};
use log::{error, info, LevelFilter};
use log4rs::{
    append::{
        console::{ConsoleAppender, Target},
        file::FileAppender,
    },
    config::{Appender, Config, Root},
    filter::threshold::ThresholdFilter,
};

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

            let player_process = runner.run(p1_stdin, p1_stdout);

            let player_pid = match player_process {
                Ok(pid) => pid,
                Err(err) => {
                    return create_error_response(&game_request, err);
                }
            };

            let simulator = simulator::Simulator::new(game_request.game_id.to_string());
            let sim_process = simulator.run(p2_stdin, p2_stdout);

            let sim_pid = match sim_process {
                Ok(pid) => pid,
                Err(err) => {
                    return create_error_response(&game_request, err);
                }
            };

            let player_process_out =
                cc_driver::handle_process(player_pid, true, SimulatorError::RuntimeError);

            if let Err(err) = player_process_out {
                error!("Error from player.");
                return create_error_response(&game_request, err);
            }

            let player_process_out = player_process_out.unwrap();

            let sim_process_out =
                cc_driver::handle_process(sim_pid, false, SimulatorError::RuntimeError);

            if let Err(err) = sim_process_out {
                error!("Error from simulator.");
                return create_error_response(&game_request, err);
            }

            let sim_process_out = sim_process_out.unwrap();

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
            println!("{}", e);
        }
    }
}
