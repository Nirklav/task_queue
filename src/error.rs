//! TaskQueue error.

use std::error::Error;
use std::fmt:: { Display, Formatter };
use std::fmt;
use std::option::Option;
use std::io;

#[derive(Debug)]
pub enum TaskQueueError {
    Io(io::Error),
    Join,
}

impl Error for TaskQueueError {
    fn description(&self) -> &str {
        match *self {
            TaskQueueError::Io(ref e) => e.description(),
            TaskQueueError::Join => "Error on JoinHandle::Error invoke",
        }
    }

    fn cause(&self) -> Option<&Error> {
        match *self {
            TaskQueueError::Io(ref e) => Some(e),
            TaskQueueError::Join => None,
        }
    }
}

impl Display for TaskQueueError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match *self {
            TaskQueueError::Io(ref err) => write!(f, "IO error: {}", err),
            TaskQueueError::Join => write!(f, "Join error: {}", self.description()),
        }
    }
}

impl From<io::Error> for TaskQueueError {
    fn from(err: io::Error) -> TaskQueueError {
        TaskQueueError::Io(err)
    }
}
