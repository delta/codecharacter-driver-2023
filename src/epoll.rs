use std::{os::{fd::{RawFd, AsRawFd}, linux::process::ChildExt}, process::Child, collections::HashMap};

use nix::{sys::epoll::{epoll_create, epoll_ctl, EpollEvent, EpollFlags, EpollOp, epoll_wait}, unistd::close};

use crate::error::EpollError;

pub struct Epoll {
    processes: HashMap<u64, u32>,
    epoll_fd: RawFd,
}

impl Epoll {
    pub fn new() -> Result<Self, EpollError> {
        // create epoll here
        let fd = epoll_create();

        match fd {
            Ok(fd) => {
                Ok(Epoll { processes: HashMap::new(), epoll_fd: fd })
            },
            Err(_) => {
                Err(EpollError::EpollCreateError("Unable to create epoll".to_owned()))
            }
        }
    }

    pub fn register(&mut self, process: &Child) -> Result<(), EpollError> {
        let fd = process.pidfd();

        if fd.is_err() {
            return Err(EpollError::PidFdError("Unable to retrieve pid_fd".to_owned()));
        }

        let fd = fd.unwrap().as_raw_fd() as u64;
        let mut epoll_event = EpollEvent::new( EpollFlags::EPOLLIN, fd);

        if epoll_ctl(self.epoll_fd, EpollOp::EpollCtlAdd, fd as i32, &mut epoll_event).is_err() {
            return Err(EpollError::EpollRegisterError("Unable to register the process".to_owned()));
        }

        println!("Insertin key {fd}, value: {}", process.id());
        self.processes.insert(fd, process.id());
        Ok(())
    }

    pub fn on_event<T, K>(&mut self, timeout: isize, callback: T) -> Result<K, EpollError>
    where T: Fn(u32, u64, &Vec<u32>) -> K {
        // array with events

        // => create Epoll object,
        // => register one or more processes,
        // => wait termination will wait for events
        let mut events = [EpollEvent::new(EpollFlags::EPOLLIN, 0); 1];
        let event_count = epoll_wait(self.epoll_fd, &mut events, timeout);

        if event_count.is_err() {
            return Err(EpollError::EpollWaitError("Cannot not listen for epoll events".to_owned()));
        }

        let _event_count = event_count.unwrap();

        // (0..event_count).for_each(|i| {
        let event = events[0];
        println!("Event occured on fd: {:?}", event.data());
        let fd = event.data();
        let pid = self.processes.get(&fd).unwrap();
        let processes = &self.processes.values().cloned().collect();

        Ok(callback(*pid, fd, processes))

        // });

        // let global
        // lambda () => global = mkilled;
        // if killed return smthn

        // for event in events.iter() {
        //     println!("Event occured on fd: {:?}", event.data());
        //     let fd = event.data();
        //     let pid = self.processes.get(&fd).unwrap();
        //     let processes = &self.processes.values().cloned().collect();

        //     callback(*pid, fd, processes);
        // }
    }
}

impl Drop for Epoll {
    fn drop(&mut self) {
        let _ = close(self.epoll_fd);
    }
}