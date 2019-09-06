use std::fmt;
use std::io;
use winrt;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Rt(winrt::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::Io(ref err) => write!(f, "I/O error: {}", err),
            Error::Rt(ref err) => write!(f, "WinRT error: {:?}", err),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match *self {
            Error::Io(ref err) => Some(err),
            Error::Rt(_) => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Error {
        Error::Io(error)
    }
}

impl From<winrt::Error> for Error {
    fn from(error: winrt::Error) -> Error {
        Error::Rt(error)
    }
}
