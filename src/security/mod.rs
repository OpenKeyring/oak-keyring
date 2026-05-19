pub mod crash_handler;
pub mod memory;
pub mod process;

#[cfg(test)]
mod crash_handler_test;
#[cfg(test)]
mod memory_test;

pub use memory::{LockedKey32, LockedSecretBytes};
pub use process::{apply_process_protections, ProcessProtections};
