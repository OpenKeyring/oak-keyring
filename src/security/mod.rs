pub mod memory;
pub mod process;

#[cfg(test)]
mod memory_test;

pub use memory::{LockedKey32, LockedSecretBytes};
pub use process::{apply_process_protections, ProcessProtections};
