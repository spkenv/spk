use crate::encoding;

/// An error returned by the graph module
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    UnknownObject(UnknownObjectError),
    UnknownReference(UnknownReferenceError),
    AmbiguousReference(AmbiguousReferenceError),
    InvalidReferenceError(InvalidReferenceError),
}

impl From<UnknownObjectError> for Error {
    fn from(err: UnknownObjectError) -> Self {
        Error::UnknownObject(err)
    }
}
impl From<UnknownReferenceError> for Error {
    fn from(err: UnknownReferenceError) -> Self {
        Error::UnknownReference(err)
    }
}
impl From<AmbiguousReferenceError> for Error {
    fn from(err: AmbiguousReferenceError) -> Self {
        Error::AmbiguousReference(err)
    }
}

/// Denotes a missing object or one that is not present in the database.
#[derive(Debug, Eq, PartialEq)]
pub struct UnknownObjectError {
    message: String,
}

impl UnknownObjectError {
    pub fn new(digest: encoding::Digest) -> Self {
        Self {
            message: format!("Unknown object: {}", digest),
        }
    }
}

/// Denotes a reference that is not known.
#[derive(Debug, Eq, PartialEq)]
pub struct UnknownReferenceError {
    message: String,
}

impl UnknownReferenceError {
    pub fn new(reference: String) -> Self {
        Self {
            message: format!("Unknown reference: {}", reference),
        }
    }
}

/// Denotes a reference that could refer to more than one object in the storage.
#[derive(Debug, Eq, PartialEq)]
pub struct AmbiguousReferenceError {
    message: String,
}

impl AmbiguousReferenceError {
    pub fn new(reference: String) -> Self {
        Self {
            message: format!("Ambiguous reference [too short]: {}", reference),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Denotes a missing object or one that is not present in the database.
#[derive(Debug, Eq, PartialEq)]
pub struct InvalidReferenceError {
    message: String,
}

impl InvalidReferenceError {
    pub fn new(reference: impl Into<String>) -> Self {
        Self {
            message: format!("Invalid reference: {}", reference.into()),
        }
    }
}
