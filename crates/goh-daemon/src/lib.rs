//! Local completion daemon (docs/04, docs/06): framed JSON over a user-private Unix socket.

pub mod autospec;
pub mod client;
pub mod config;
pub mod control;
pub mod doctor;
pub mod helpgen;
pub mod log;
mod packages;
pub mod paths;
pub mod probe;
mod project;
mod providers;
pub mod relay;
pub mod risk;
pub mod runs;
mod shell;
pub mod snippets;
pub mod server;
pub mod setup;
pub mod sources;
pub mod specs;
pub mod update;
pub mod usage;
pub mod visits;
pub mod wire;
