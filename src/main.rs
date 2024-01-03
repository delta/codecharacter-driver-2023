use std::{env, sync::Arc};

use cc_driver::{
    create_executing_response,
    handlers::Handler,
    mq::{consumer, Publisher},
    request::GameRequest,
};
use log::LevelFilter;
use log4rs::{
    append::{
        console::{ConsoleAppender, Target},
        file::FileAppender,
    },
    config::{Appender, Config, Root},
    filter::threshold::ThresholdFilter,
};

fn worker_fn(msg_receiver: crossbeam_channel::Receiver<GameRequest>, publisher: Arc<Publisher>) {
    while let Ok(req) = msg_receiver.recv() {
        // publishing error means we can crash, something is wrong
        publisher
            .publish(create_executing_response(req.game_id()))
            .unwrap();
        let response = req.handle();
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
        env::var("RABBIT_MQ_HOST").unwrap(),
        env::var("NORMAL_GAME_REQUEST_QUEUE").unwrap(),
        env::var("PVP_GAME_REQUEST_QUEUE").unwrap(),
        env::var("GAME_RESPONSE_QUEUE").unwrap(),
        worker_fn,
    );

    match res {
        Ok(_) => {}
        Err(e) => {
            log::error!("{e}");
        }
    }
}
