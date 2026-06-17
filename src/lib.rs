pub mod aircraft;
pub mod config;
pub mod db;
pub mod geo;
pub mod sbs1;
pub mod utils;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unknown frame type")]
    UnknownFrame,
    #[error("incomplete frame")]
    IncompleteFrame,
    #[error("connection closed")]
    ConnectionClosed,
    #[error("connection reset")]
    ConnectionReset,
    #[error("parse error")]
    ParseError,
    #[error("invalid input")]
    InvalidInput,
}
