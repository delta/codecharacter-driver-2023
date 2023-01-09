use crate::MAXLOGSIZE;

use std::io::Read;
use std::process::ChildStderr;

use crate::error::SimulatorError;

use std::process::ExitStatus;

use std::process::Child;

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
        Process { process: proc, process_type: proc_type }
    }

    pub fn get_process(&self) -> &Child {
        &self.process
    }

    pub fn get_type(&self) -> &ProcessType {
        &self.process_type
    }

    pub fn wait(&mut self) -> Result<ExitStatus, SimulatorError> {
        self.process.wait()
            .map_err(|err| SimulatorError::UnidentifiedError(
                format!("Waiting on Child Failed: {err}")))
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
        ProcessOutput { stderr, output: String::new(), process_type: proc_type }
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
        let map_err = |err| SimulatorError::UnidentifiedError(format!("Error during log extraction: {err}"));

        match self.process_type {
            ProcessType::Runner => {
                let limit = MAXLOGSIZE - self.output.len();
                let stderr = &mut self.stderr;

                let _ = stderr
                    .take(limit as u64)
                    .read_to_string(&mut buf)
                    .map_err(map_err)?;
            },
            ProcessType::Simulator => {
                let _ = self.stderr
                    .read_to_string(&mut buf)
                    .map_err(map_err)?;
            }
        }

        self.output.push_str(buf.as_str());
        Ok(())
    }
}

pub enum Files {
    Process(Process),
    StdErr(ProcessOutput)
}
