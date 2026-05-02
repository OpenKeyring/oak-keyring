//! Onboarding wizard — 3-path setup flow with step management.
//!
//! Paths: CreateNew (create vault + recovery key), Restore (recovery key restore),
//! Import (import from other manager). Each path has its own step sequence.

pub mod screen;
#[cfg(test)]
pub mod tests;
pub mod types;
pub mod views;

pub use screen::OnboardingScreen;
pub use types::OnboardingPath;
