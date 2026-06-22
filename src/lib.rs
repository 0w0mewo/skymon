use std::io;

pub mod aircraft;
pub mod feeders;
pub mod config;
pub mod database;
pub mod utils;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unknown frame type")]
    UnknownFrame,
    #[error("incomplete frame")]
    IncompleteFrame,
    #[error("connection closed")]
    ConnectionClosed(#[from] io::Error),
    #[error("parse error")]
    ParseError,
    #[error("invalid input")]
    InvalidInput,
}
