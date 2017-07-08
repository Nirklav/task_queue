//! TaskQueue error.

use std::error::Error;
use std::fmt:: { Display, Formatter };
use std::fmt;
use std::option::Option;
use std::io;

#[derive(Debug)]
pub enum TaskQueueError {
    IllegalStartThreads {
        min: usize,
        max: usize
    },
    Io(io::Error),
    IllegalPolicyThreads {
        min:usize,
        max:usize,
        count:usize
    }
}

impl TaskQueueError {
    pub fn illegal_start_threads(min: usize, max: usize) -> TaskQueueError {
        TaskQueueError::IllegalStartThreads {
            min,
            max
        }
    }

    pub fn illegal_policy_threads(min: usize, max: usize, count: usize) -> TaskQueueError {
        TaskQueueError::IllegalPolicyThreads {
            min,
            max,
            count
        }
    }
}

impl Error for TaskQueueError {
    fn description(&self) -> &str {
        match self {
            &TaskQueueError::IllegalStartThreads { .. } => "Illegal number of threads was received",
            &TaskQueueError::Io(ref e) => e.description(),
            &TaskQueueError::IllegalPolicyThreads { .. }  => "Policy was returned illegal number of threads",

        }
    }

    fn cause(&self) -> Option<&Error> {
        match self {
            &TaskQueueError::IllegalStartThreads { .. } => None,
            &TaskQueueError::Io(ref e) => Some(e),
            &TaskQueueError::IllegalPolicyThreads { .. } => None,
        }
    }
}

impl Display for TaskQueueError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            &TaskQueueError::IllegalStartThreads { min, max } => write!(f, "Illegal number of threads was received min:{} max:{}", min, max),
            &TaskQueueError::Io(ref err) => write!(f, "IO error: {}", err),
            &TaskQueueError::IllegalPolicyThreads { min, max, count } => write!(f, "Policy returned illegal number of threads min:{} max:{} count:{}", min, max, count)
        }
    }
}

impl From<io::Error> for TaskQueueError {
    fn from(err: io::Error) -> TaskQueueError {
        TaskQueueError::Io(err)
    }
}
