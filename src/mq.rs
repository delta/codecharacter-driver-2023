use std::sync::{Arc, Mutex};

use crate::{
    error::SimulatorError,
    request::{GameRequest, NormalGameRequest, PvPGameRequest},
    response::GameStatus,
};

use amiquip::{
    Channel, Connection, ConsumerMessage, ConsumerOptions, Exchange, Publish, QueueDeclareOptions,
    Result,
};
use crossbeam_channel::Sender;

const NUM_OF_THREADS: usize = 2;

pub fn listen<T>(
    url: String,
    consumer_queue_name: String,
    s: Sender<GameRequest>,
) -> Result<(), SimulatorError>
where
    T: for<'a> serde::Deserialize<'a> + Into<GameRequest>,
{
    let mut connection = Connection::insecure_open(&url).map_err(|e| {
        SimulatorError::RabbitMqError(format!(
            "Error in opening connection to request queue [Connection::insecure_open]: {e:?}"
        ))
    })?;

    let channel = connection.open_channel(None).map_err(|e| {
        SimulatorError::RabbitMqError(format!(
            "Error in opening request channel [Connection::open_channel]: {e:?}"
        ))
    })?;

    let queue = channel
        .queue_declare(
            &consumer_queue_name,
            QueueDeclareOptions {
                durable: true,
                ..Default::default()
            },
        )
        .map_err(|e| {
            SimulatorError::UnidentifiedError(format!("Error in requesting to the queue: {e:?}"))
        })?;

    let consumer = queue.consume(ConsumerOptions::default()).map_err(|e| {
        SimulatorError::RabbitMqError(format!(
            "Error in publishing to the queue [Publisher::new]: {e:?}"
        ))
    })?;

    for message in consumer.receiver().iter() {
        match message {
            ConsumerMessage::Delivery(delivery) => {
                let body_str = String::from_utf8_lossy(&delivery.body);
                let res: Result<T, serde_json::Error> = serde_json::from_str(&body_str);

                consumer.ack(delivery).map_err(|e| {
                    SimulatorError::RabbitMqError(format!("Unable to send acknowledgement {e:?}"))
                })?;

                match res {
                    Ok(match_request) => {
                        s.send(match_request.into()).unwrap();
                    }
                    Err(e) => {
                        log::error!("{e:?}");
                    }
                }
            }
            e => {
                log::error!("{e:?}");
            }
        }
    }

    Ok(())
}

pub fn consumer(
    url: String,
    normal_game_consumer_queue_name: String,
    pvp_game_consumer_queue_name: String,
    response_producer_queue_name: String,
    handler_fn: fn(crossbeam_channel::Receiver<GameRequest>, Arc<Publisher>) -> (),
) -> amiquip::Result<()> {
    let response_publisher =
        Arc::new(Publisher::new(url.to_owned(), response_producer_queue_name).unwrap());

    let (s, r) = crossbeam_channel::bounded(NUM_OF_THREADS + 1);

    // each thread has a receiver
    let mut threads = vec![];
    for _ in 0..NUM_OF_THREADS {
        let new_r = r.clone();
        let publisher_clone = Arc::clone(&response_publisher);
        threads.push(std::thread::spawn(move || {
            handler_fn(new_r, publisher_clone)
        }))
    }

    let pvp_s = s.clone();
    let url_ = url.clone();

    let _ = std::thread::spawn(move || {
        listen::<NormalGameRequest>(url, normal_game_consumer_queue_name, s)
    });

    let _ = listen::<PvPGameRequest>(url_, pvp_game_consumer_queue_name, pvp_s);

    Ok(())
}

pub struct Publisher {
    connection: Option<Connection>,
    channel: Mutex<Channel>,
    queue_name: String,
}

impl Publisher {
    pub fn new(url: String, queue_name: String) -> Result<Self, SimulatorError> {
        let mut connection = Connection::insecure_open(&url).map_err(|e| {
            SimulatorError::RabbitMqError(format!(
                "Error in opening connection to publish queue [Connection::insecure_open]: {e}"
            ))
        })?;

        let channel = connection.open_channel(None).map_err(|e| {
            SimulatorError::RabbitMqError(format!(
                "Error in opening publish channel [Connection::open_channel]: {e}"
            ))
        })?;

        channel
            .queue_declare(
                &queue_name,
                QueueDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
            )
            .map_err(|e| {
                SimulatorError::RabbitMqError(format!(
                    "Error in publishing to the queue [Publisher::new]: {e}"
                ))
            })?;

        Ok(Self {
            connection: Some(connection),
            channel: Mutex::new(channel),
            queue_name,
        })
    }
    pub fn publish(&self, response: GameStatus) -> Result<(), SimulatorError> {
        let channel = self.channel.lock().unwrap();
        let exchange = Exchange::direct(&channel);
        let body = serde_json::to_string(&response)
            .map_err(|e| SimulatorError::UnidentifiedError(format!("{e}")))?;
        exchange
            .publish(Publish::new(body.as_bytes(), &self.queue_name))
            .map_err(|e| {
                SimulatorError::UnidentifiedError(format!(
                    "Error in publishing to the queue[Publisher::publish]{e}"
                ))
            })?;
        Ok(())
    }
}

impl Drop for Publisher {
    fn drop(&mut self) {
        if self.connection.is_some() {
            let conn = self.connection.take().unwrap();
            let _ = conn.close();
        }
    }
}
