use std::io;

#[derive(Debug)]
pub enum Error {
    String(String),
    Nix(nix::Error),
    IO(io::Error),
    JSON(serde_json::Error),
}

impl Error {
    pub fn new<S: AsRef<str>>(message: S) -> Error {
        Error::new_io(io::ErrorKind::Other, message.as_ref())
    }

    pub fn new_io<E: Into<Box<dyn std::error::Error + Send + Sync>>>(
        kind: io::ErrorKind,
        e: E,
    ) -> Error {
        Error::IO(io::Error::new(kind, e))
    }

    pub fn raw_os_error(&self) -> Option<i32> {
        match self {
            Error::IO(err) => err.raw_os_error(),
            Error::Nix(err) => {
                let errno = err.as_errno();
                if let Some(e) = errno {
                    return Some(e as i32);
                }
                None
            }
            _ => None,
        }
    }
}

impl From<nix::Error> for Error {
    fn from(err: nix::Error) -> Error {
        Error::Nix(err)
    }
}
impl From<nix::errno::Errno> for Error {
    fn from(errno: nix::errno::Errno) -> Error {
        Error::Nix(nix::Error::from_errno(errno))
    }
}
impl From<i32> for Error {
    fn from(errno: i32) -> Error {
        Error::IO(std::io::Error::from_raw_os_error(errno))
    }
}
impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::IO(err)
    }
}
impl From<String> for Error {
    fn from(err: String) -> Error {
        Error::String(err)
    }
}
impl From<&str> for Error {
    fn from(err: &str) -> Error {
        Error::String(err.to_string())
    }
}
impl From<std::path::StripPrefixError> for Error {
    fn from(err: std::path::StripPrefixError) -> Self {
        Error::String(err.to_string())
    }
}
impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::JSON(err)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
