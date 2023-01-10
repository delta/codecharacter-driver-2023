#[derive(Debug)]
pub enum SimulatorError {
    CompilationError(String),
    RuntimeError(String),
    UnidentifiedError(String),
    FifoCreationError(String),
    EpollError(String),
    TimeOutError(String),
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
        // match val {
        // EpollError::EpollCreateError(e) => SimulatorError::EpollError(e),
        // EpollError::PidFdError(e) => SimulatorError::EpollError(e),
        // EpollError::EpollRegisterError(e) => SimulatorError::EpollError(e),
        // EpollError::EpollWaitError(e) => SimulatorError::EpollError(e),
        // EpollError::EpollProcessNotFound(e) => SimulatorError::EpollError(e),
        // }
    }
}

