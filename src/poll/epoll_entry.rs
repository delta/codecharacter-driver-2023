use nix::sys::epoll::EpollFlags;

use crate::error::EpollError;

use std::env;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::linux::process::ChildExt;
use std::process::ChildStderr;

use crate::error::SimulatorError;

use std::process::ExitStatus;

use std::process::Child;

use super::epoll::CallbackMessage;
use super::epoll::Pollable;

#[derive(Debug)]
pub enum ProcessType {
    Runner,
    Simulator,
}

#[derive(Debug)]
pub struct Process {
    process: Child,
    process_type: ProcessType,
}

impl Process {
    pub fn new(proc: Child, proc_type: ProcessType) -> Self {
        Process {
            process: proc,
            process_type: proc_type,
        }
    }

    pub fn get_process(&self) -> &Child {
        &self.process
    }

    pub fn get_type(&self) -> &ProcessType {
        &self.process_type
    }

    pub fn wait(&mut self) -> Result<ExitStatus, SimulatorError> {
        self.process.wait().map_err(|err| {
            SimulatorError::UnidentifiedError(format!("Waiting on Child Failed: {err}"))
        })
    }

    pub fn kill(&mut self) {
        let _ = self.process.kill();
    }
}

pub struct ProcessOutput {
    stderr: ChildStderr,
    output: String,
    process_type: ProcessType,
}

impl ProcessOutput {
    pub fn new(stderr: ChildStderr, proc_type: ProcessType) -> Self {
        ProcessOutput {
            stderr,
            output: String::new(),
            process_type: proc_type,
        }
    }

    pub fn stderr(&self) -> &ChildStderr {
        &self.stderr
    }

    pub fn output(self) -> String {
        self.output
    }

    pub fn process_type(&self) -> &ProcessType {
        &self.process_type
    }

    pub fn read_to_string(&mut self) -> Result<(), SimulatorError> {
        let mut buf = String::new();
        let map_err =
            |err| SimulatorError::UnidentifiedError(format!("Error during log extraction: {err}"));

        match self.process_type {
            ProcessType::Runner => {
                let limit: usize = env::var("MAX_LOG_SIZE").unwrap().parse().unwrap();
                let stderr = &mut self.stderr;

                let _ = stderr
                    .take(limit as u64)
                    .read_to_string(&mut buf)
                    .map_err(map_err)?;
                if self.output.len() < limit {
                    let rem = limit - self.output.len();
                    buf = buf.chars().take(rem).collect::<String>();
                } else {
                    buf = String::new();
                }
            }
            ProcessType::Simulator => {
                let _ = self.stderr.read_to_string(&mut buf).map_err(map_err)?;
            }
        }

        self.output.push_str(buf.as_str());
        Ok(())
    }
}

pub enum EpollEntryType {
    Process(Process),
    StdErr(ProcessOutput),
}

impl Pollable for EpollEntryType {
    fn get_fd(&self) -> std::os::fd::RawFd {
        match self {
            EpollEntryType::Process(p) => p
                .get_process()
                .pidfd()
                .expect("PidFd should be extractable from Child, make sure the Command is invoked correctly")
                .as_raw_fd(),
            EpollEntryType::StdErr(e) => e.stderr().as_raw_fd(),
        }
    }
    fn process_event(
        &mut self,
        event: nix::sys::epoll::EpollEvent,
    ) -> Result<CallbackMessage, EpollError> {
        let fd = event.data();
        let flags = event.events();
        match self {
            EpollEntryType::Process(_) => Ok(CallbackMessage::HandleExplicitly(self.get_fd())),
            EpollEntryType::StdErr(output) => {
                let mut message = CallbackMessage::Nop;
                if flags.contains(EpollFlags::EPOLLIN) {
                    output
                        .read_to_string()
                        .map_err(|e| EpollError::EpollCallbackError(format!("{e:?}")))?;
                }
                if flags.contains(EpollFlags::EPOLLHUP) {
                    message = CallbackMessage::Unregister(fd as i32);
                }
                Ok(message)
            }
        }
    }
}
