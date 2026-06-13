pub mod discovery;
pub mod handler;
pub mod identity;
pub mod listener;
pub mod pairing;
pub mod rpc;
pub mod session;
pub mod tls;
pub mod transport;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("identity error: {0}")]
    Identity(String),
    #[error("TLS error: {0}")]
    Tls(String),
    #[error("discovery error: {0}")]
    Discovery(String),
    #[error("pairing error: {0}")]
    Pairing(String),
    #[error("transport error: {0}")]
    Transport(String),
    #[error("RPC error: {0}")]
    Rpc(String),
    #[error("session error: {0}")]
    Session(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
