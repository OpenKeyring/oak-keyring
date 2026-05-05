//! OpVault (1Password .opvault) format parser.

pub mod crypto;
pub mod parser;
pub mod types;

#[cfg(test)]
mod crypto_test;

#[cfg(test)]
mod integration_test;
