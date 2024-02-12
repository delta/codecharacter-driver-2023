#[derive(Debug,Clone)]
pub enum SimulatorError {
    CompilationError(String),
    RuntimeError(String),
    UnidentifiedError(String),
    FifoCreationError(String),
    EpollError(String),
    TimeOutError(String),
    RabbitMqError(String),
    Player1Error(String),
    Player2Error(String),
}

#[derive(Debug)]
pub enum EpollError {
    EpollCallbackError(String),
    EpollFdError(String),
    EpollCreateError(String),
    PidFdError(String),
    EpollRegisterError(String),
    EpollWaitError(String),
    EpollProcessNotFound(String),
}

impl From<EpollError> for SimulatorError {
    fn from(val: EpollError) -> Self {
        SimulatorError::EpollError(format!("{val:?}"))
    }
}
