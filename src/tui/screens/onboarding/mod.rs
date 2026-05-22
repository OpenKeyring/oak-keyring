//! Onboarding wizard — 3-path setup flow with step management.
//!
//! Paths: CreateNew (create vault + recovery key), Restore (recovery key restore),
//! Import (import from other manager). Each path has its own step sequence.

mod handlers;
mod logo;
pub mod screen;
#[cfg(test)]
pub mod tests;
mod types;
mod views_import;
mod views_recovery;
mod views_setup;

pub use screen::OnboardingScreen;
pub use types::*;
