use std::collections::HashMap;
use std::os::fd::RawFd;

use nix::{
    sys::epoll::{epoll_create, epoll_ctl, epoll_wait, EpollEvent, EpollFlags, EpollOp},
    unistd::close,
};

use crate::error::{EpollError, SimulatorError};

pub trait Pollable {
    fn get_fd(&self) -> RawFd;
    fn process_event(&mut self, flags: EpollEvent) -> Result<CallbackMessage, EpollError>;
}

pub struct EpollGeneric<T: Pollable> {
    fds: HashMap<u64, T>,
    epoll_fd: RawFd,
}

impl<T: Pollable> EpollGeneric<T> {
    pub fn new() -> Result<Self, EpollError> {
        let fd = epoll_create()
            .map_err(|_| EpollError::EpollCreateError("Unable to create epoll".to_owned()))?;

        Ok(EpollGeneric {
            fds: HashMap::new(),
            epoll_fd: fd,
        })
    }

    pub fn get_registered_fds(&self) -> &HashMap<u64, T> {
        &self.fds
    }

    pub fn is_empty(&self) -> bool {
        self.fds.is_empty()
    }

    fn register_fd(&mut self, fd: i32, flags: EpollFlags) -> Result<(), EpollError> {
        let mut epoll_event = EpollEvent::new(flags, fd as u64);

        epoll_ctl(self.epoll_fd, EpollOp::EpollCtlAdd, fd, &mut epoll_event).map_err(|_| {
            EpollError::EpollRegisterError("Unable to register the process".to_owned())
        })?;

        Ok(())
    }

    fn unregister_fd(&self, fd: i32) -> Result<(), EpollError> {
        let mut epoll_event = EpollEvent::new(EpollFlags::EPOLLIN, fd as u64);

        epoll_ctl(self.epoll_fd, EpollOp::EpollCtlDel, fd, &mut epoll_event).map_err(|_| {
            EpollError::EpollRegisterError("Unable to unregister the process".to_owned())
        })?;

        Ok(())
    }
    pub fn register(&mut self, entry: T, flags: EpollFlags) -> Result<(), EpollError> {
        let fd = entry.get_fd();
        self.register_fd(fd, flags)?;
        self.fds.insert(fd as u64, entry);
        Ok(())
    }

    pub fn unregister(&mut self, fd: u64) -> Result<T, EpollError> {
        if !self.fds.contains_key(&fd) {
            return Err(EpollError::EpollFdError(
                "Fd is not registered for epoll".to_owned(),
            ));
        }
        self.unregister_fd(fd as i32)?;
        self.fds.remove(&fd).ok_or(EpollError::EpollFdError(
            "Fd is not registered for epoll".to_owned(),
        ))
    }

    pub fn poll(
        &mut self,
        timeout: isize,
        maxevents: usize,
    ) -> Result<Vec<EpollEvent>, EpollError> {
        let mut events = vec![EpollEvent::new(EpollFlags::EPOLLIN, 0); maxevents];
        let event_count = epoll_wait(self.epoll_fd, &mut events, timeout).map_err(|_| {
            EpollError::EpollWaitError("Unable to listen for epoll events".to_owned())
        })?;
        Ok(events[..event_count].to_vec())
    }
    pub fn process_event(&mut self, event: EpollEvent) -> Result<CallbackMessage, SimulatorError> {
        let fd = event.data();
        match self.fds.get_mut(&fd) {
            Some(handle) => Ok(handle.process_event(event)?),
            None => Ok(CallbackMessage::Nop),
        }
    }
}

pub enum CallbackMessage {
    Unregister(i32),
    HandleExplicitly(i32),
    Nop,
}

impl<T: Pollable> Drop for EpollGeneric<T> {
    fn drop(&mut self) {
        let _ = close(self.epoll_fd);
    }
}
