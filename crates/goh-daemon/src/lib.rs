//! Local completion daemon (docs/04, docs/06): framed JSON over a user-private Unix socket.

pub mod client;
pub mod paths;
pub mod probe;
pub mod server;
pub mod wire;
