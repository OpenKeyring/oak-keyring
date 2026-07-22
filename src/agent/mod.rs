//! SSH agent backend (`ok agent`).
//!
//! Implements an in-process ssh-agent compatible server backed by the vault's
//! SSH records. This task lands the signer layer only; identity, server, cli,
//! paths and lock modules are added in subsequent tasks.

pub mod signer;
