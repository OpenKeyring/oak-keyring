//! SSH agent backend (`ok agent`).
//!
//! Implements an in-process ssh-agent compatible server backed by the vault's
//! SSH records. This task lands the signer and identity layers; server, cli,
//! paths and lock modules are added in subsequent tasks.

pub mod cli;
pub mod identity;
pub mod lock;
pub mod paths;
pub mod server;
pub mod signer;
