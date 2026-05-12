pub mod memory;

#[cfg(test)]
mod memory_test;

pub use memory::{LockedKey32, LockedSecretBytes};
