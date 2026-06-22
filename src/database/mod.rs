pub mod db;
pub mod models;

use std::io;

use rusqlite::{self as rqlite};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error")]
    DatabaseErr(#[from] rqlite::Error),
    #[error("invalid input")]
    InvalidInput,
    #[error("unknown error: {0}")]
    Unknown(String),
    #[error("io error")]
    IOError(#[from] io::Error)
}

pub type QueryResult<T> = Result<T, Error>;