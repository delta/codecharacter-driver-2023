use std::{
    collections::HashMap,
    os::{
        fd::{AsRawFd, RawFd},
        linux::process::ChildExt,
    },
};

use nix::{
    sys::epoll::{epoll_create, epoll_ctl, epoll_wait, EpollEvent, EpollFlags, EpollOp},
    unistd::close,
};

use super::epoll_entry::{EpollEntryType, Process, ProcessOutput};
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

pub struct Epoll {
    fds: HashMap<u64, EpollEntryType>,
    epoll_fd: RawFd,
}

impl Epoll {
    pub fn new() -> Result<Self, EpollError> {
        let fd = epoll_create()
            .map_err(|_| EpollError::EpollCreateError("Unable to create epoll".to_owned()))?;

        Ok(Epoll {
            fds: HashMap::new(),
            epoll_fd: fd,
        })
    }

    fn register_fd(&mut self, fd: i32) -> Result<(), EpollError> {
        let mut epoll_event =
            EpollEvent::new(EpollFlags::EPOLLIN | EpollFlags::EPOLLHUP, fd as u64);

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

    pub fn register(&mut self, file: EpollEntryType) -> Result<(), EpollError> {
        match file {
            EpollEntryType::Process(proc) => {
                let fd = proc
                    .get_process()
                    .pidfd()
                    .map_err(|_| {
                        EpollError::PidFdError("Unable to retrieve pid_fd of process".to_owned())
                    })?
                    .as_raw_fd();

                self.register_fd(fd)?;

                self.fds.insert(fd as u64, EpollEntryType::Process(proc));
            }
            EpollEntryType::StdErr(stderr) => {
                let fd = stderr.stderr().as_raw_fd();
                self.register_fd(fd)?;

                self.fds.insert(fd as u64, EpollEntryType::StdErr(stderr));
            }
        };

        Ok(())
    }

    pub fn unregister(
        &mut self,
        fd: u64,
    ) -> Result<(Option<Process>, Option<ProcessOutput>), EpollError> {
        if !self.fds.contains_key(&fd) {
            return Err(EpollError::EpollProcessNotFound(
                "Process not registered for epoll".to_owned(),
            ));
        }

        self.unregister_fd(fd as i32)?;

        let files = self.fds.remove(&fd).unwrap();

        match files {
            EpollEntryType::Process(process) => Ok((Some(process), None)),
            EpollEntryType::StdErr(stderr) => Ok((None, Some(stderr))),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.fds.is_empty()
    }

    pub fn clear_processes(&mut self) -> Vec<Process> {
        let keys: Vec<u64> = self
            .fds
            .keys()
            .filter(|x| match self.fds.get(*x).unwrap() {
                EpollEntryType::Process(_) => true,
                EpollEntryType::StdErr(_) => false,
            })
            .copied()
            .collect();

        keys.iter()
            .map(|key| {
                let file = self.fds.remove(key).unwrap();
                match file {
                    EpollEntryType::Process(proc) => proc,
                    _ => panic!(),
                }
            })
            .collect()
    }

    pub fn poll(
        &mut self,
        timeout: isize,
    ) -> Result<Option<(EpollEvent, &mut EpollEntryType)>, EpollError> {
        let mut events = vec![EpollEvent::new(EpollFlags::EPOLLIN, 0); self.fds.len()];
        let event_count = epoll_wait(self.epoll_fd, &mut events, timeout).map_err(|_| {
            EpollError::EpollWaitError("Unable to listen for epoll events".to_owned())
        })?;

        if event_count == 0 {
            return Ok(None);
        }

        let event = events.first().unwrap();
        let fd = event.data();
        Ok(Some((*event, self.fds.get_mut(&fd).unwrap())))
    }
}

impl Drop for Epoll {
    fn drop(&mut self) {
        let _ = close(self.epoll_fd);
    }
}
