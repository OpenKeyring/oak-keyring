use crate::errors::mapping::sync::SyncError;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncTrigger {
    TriggerSync,
    PullOnly,
    PullCompleted,
    DetectCompleted {
        has_conflicts: bool,
        has_changes: bool,
    },
    PushCompleted {
        has_conflicts: bool,
    },
    AllConflictsResolved,
    ConflictResolutionFailed,
    NetworkError,
    OtherError,
    BackoffExpired,
    MaxRetriesExceeded,
    Shutdown,
    ReportCompleted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncState {
    Idle,
    Pulling,
    Detecting,
    Pushing,
    Resolving,
    Synced,
    Error,
    Offline,
    ShuttingDown,
}

impl fmt::Display for SyncState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SyncState::Idle => write!(f, "Idle"),
            SyncState::Pulling => write!(f, "Pulling"),
            SyncState::Detecting => write!(f, "Detecting"),
            SyncState::Pushing => write!(f, "Pushing"),
            SyncState::Resolving => write!(f, "Resolving"),
            SyncState::Synced => write!(f, "Synced"),
            SyncState::Error => write!(f, "Error"),
            SyncState::Offline => write!(f, "Offline"),
            SyncState::ShuttingDown => write!(f, "ShuttingDown"),
        }
    }
}

pub struct SyncStateMachine {
    state: SyncState,
    attempt: u32,
    max_retries: u32,
    last_error: Option<String>,
}

impl SyncStateMachine {
    pub fn new(max_retries: u32) -> Self {
        Self {
            state: SyncState::Idle,
            attempt: 0,
            max_retries,
            last_error: None,
        }
    }

    pub fn current_state(&self) -> &SyncState {
        &self.state
    }

    pub fn attempt(&self) -> u32 {
        self.attempt
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn reset(&mut self) {
        self.state = SyncState::Idle;
        self.attempt = 0;
        self.last_error = None;
    }

    pub fn can_accept_commands(&self) -> bool {
        matches!(
            self.state,
            SyncState::Idle | SyncState::Error | SyncState::Offline
        )
    }

    pub fn transition(&mut self, trigger: SyncTrigger) -> Result<SyncState, SyncError> {
        let to = self.compute_next_state(&trigger)?;
        self.apply_side_effects(&trigger, &to);
        self.state = to.clone();
        Ok(to)
    }

    fn compute_next_state(&self, trigger: &SyncTrigger) -> Result<SyncState, SyncError> {
        if matches!(trigger, SyncTrigger::Shutdown) {
            return Ok(SyncState::ShuttingDown);
        }

        match (&self.state, trigger) {
            (SyncState::Idle, SyncTrigger::TriggerSync) => Ok(SyncState::Pulling),
            (SyncState::Idle, SyncTrigger::PullOnly) => Ok(SyncState::Pulling),
            (SyncState::Pulling, SyncTrigger::PullCompleted) => Ok(SyncState::Detecting),
            (SyncState::Pulling, SyncTrigger::NetworkError) => Ok(SyncState::Offline),
            (SyncState::Pulling, SyncTrigger::OtherError) => Ok(SyncState::Error),
            (
                SyncState::Detecting,
                SyncTrigger::DetectCompleted {
                    has_changes: false,
                    has_conflicts: false,
                },
            ) => Ok(SyncState::Synced),
            (
                SyncState::Detecting,
                SyncTrigger::DetectCompleted {
                    has_changes: true, ..
                },
            ) => Ok(SyncState::Pushing),
            (
                SyncState::Detecting,
                SyncTrigger::DetectCompleted {
                    has_conflicts: true,
                    ..
                },
            ) => Ok(SyncState::Pushing),
            (
                SyncState::Pushing,
                SyncTrigger::PushCompleted {
                    has_conflicts: false,
                },
            ) => Ok(SyncState::Synced),
            (
                SyncState::Pushing,
                SyncTrigger::PushCompleted {
                    has_conflicts: true,
                },
            ) => Ok(SyncState::Resolving),
            (SyncState::Pushing, SyncTrigger::NetworkError) => Ok(SyncState::Offline),
            (SyncState::Pushing, SyncTrigger::OtherError) => Ok(SyncState::Error),
            (SyncState::Resolving, SyncTrigger::AllConflictsResolved) => Ok(SyncState::Synced),
            (SyncState::Resolving, SyncTrigger::ConflictResolutionFailed) => {
                Ok(SyncState::Resolving)
            }
            (SyncState::Resolving, SyncTrigger::NetworkError) => Ok(SyncState::Offline),
            (SyncState::Resolving, SyncTrigger::Shutdown) => Ok(SyncState::ShuttingDown),
            (SyncState::Synced, SyncTrigger::ReportCompleted) => Ok(SyncState::Idle),
            (SyncState::Error, SyncTrigger::BackoffExpired) if self.attempt <= self.max_retries => {
                Ok(SyncState::Pulling)
            }
            (SyncState::Error, SyncTrigger::MaxRetriesExceeded) => Ok(SyncState::Idle),
            (SyncState::Offline, SyncTrigger::BackoffExpired) => Ok(SyncState::Pulling),
            _ => Err(SyncError::InvalidStateTransition {
                from: self.state.clone().to_string(),
                to: "unknown".to_string(),
            }),
        }
    }

    fn apply_side_effects(&mut self, trigger: &SyncTrigger, to: &SyncState) {
        match (&self.state, trigger, to) {
            (SyncState::Idle, _, SyncState::Pulling) => {
                self.attempt = 0;
            }
            (SyncState::Pulling, SyncTrigger::OtherError, SyncState::Error) => {
                self.attempt += 1;
                self.last_error = Some("Sync failed due to error".to_string());
            }
            (SyncState::Pushing, SyncTrigger::OtherError, SyncState::Error) => {
                self.attempt += 1;
                self.last_error = Some("Sync failed due to error".to_string());
            }
            (SyncState::Error, SyncTrigger::MaxRetriesExceeded, SyncState::Idle) => {
                self.last_error = None;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_machine() -> SyncStateMachine {
        SyncStateMachine::new(3)
    }

    #[test]
    fn test_happy_path_idle_to_pulling() {
        let mut sm = create_machine();
        assert_eq!(*sm.current_state(), SyncState::Idle);
        assert_eq!(sm.attempt(), 0);

        let result = sm.transition(SyncTrigger::TriggerSync);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SyncState::Pulling);
        assert_eq!(*sm.current_state(), SyncState::Pulling);
        assert_eq!(sm.attempt(), 0);
    }

    #[test]
    fn test_happy_path_pulling_to_detecting() {
        let mut sm = create_machine();
        sm.transition(SyncTrigger::TriggerSync).unwrap();

        let result = sm.transition(SyncTrigger::PullCompleted);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SyncState::Detecting);
    }

    #[test]
    fn test_happy_path_detecting_to_pushing_with_changes() {
        let mut sm = create_machine();
        sm.transition(SyncTrigger::TriggerSync).unwrap();
        sm.transition(SyncTrigger::PullCompleted).unwrap();

        let result = sm.transition(SyncTrigger::DetectCompleted {
            has_changes: true,
            has_conflicts: false,
        });
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SyncState::Pushing);
    }

    #[test]
    fn test_happy_path_pushing_to_synced() {
        let mut sm = create_machine();
        sm.transition(SyncTrigger::TriggerSync).unwrap();
        sm.transition(SyncTrigger::PullCompleted).unwrap();
        sm.transition(SyncTrigger::DetectCompleted {
            has_changes: true,
            has_conflicts: false,
        })
        .unwrap();

        let result = sm.transition(SyncTrigger::PushCompleted {
            has_conflicts: false,
        });
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SyncState::Synced);
    }

    #[test]
    fn test_happy_path_synced_to_idle() {
        let mut sm = create_machine();
        sm.transition(SyncTrigger::TriggerSync).unwrap();
        sm.transition(SyncTrigger::PullCompleted).unwrap();
        sm.transition(SyncTrigger::DetectCompleted {
            has_changes: true,
            has_conflicts: false,
        })
        .unwrap();
        sm.transition(SyncTrigger::PushCompleted {
            has_conflicts: false,
        })
        .unwrap();

        let result = sm.transition(SyncTrigger::ReportCompleted);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SyncState::Idle);
    }

    #[test]
    fn test_full_happy_path() {
        let mut sm = create_machine();

        sm.transition(SyncTrigger::TriggerSync).unwrap();
        assert_eq!(*sm.current_state(), SyncState::Pulling);

        sm.transition(SyncTrigger::PullCompleted).unwrap();
        assert_eq!(*sm.current_state(), SyncState::Detecting);

        sm.transition(SyncTrigger::DetectCompleted {
            has_changes: true,
            has_conflicts: false,
        })
        .unwrap();
        assert_eq!(*sm.current_state(), SyncState::Pushing);

        sm.transition(SyncTrigger::PushCompleted {
            has_conflicts: false,
        })
        .unwrap();
        assert_eq!(*sm.current_state(), SyncState::Synced);

        sm.transition(SyncTrigger::ReportCompleted).unwrap();
        assert_eq!(*sm.current_state(), SyncState::Idle);
    }

    #[test]
    fn test_path_with_conflicts() {
        let mut sm = create_machine();

        sm.transition(SyncTrigger::TriggerSync).unwrap();
        sm.transition(SyncTrigger::PullCompleted).unwrap();
        sm.transition(SyncTrigger::DetectCompleted {
            has_changes: true,
            has_conflicts: true,
        })
        .unwrap();
        assert_eq!(*sm.current_state(), SyncState::Pushing);

        sm.transition(SyncTrigger::PushCompleted {
            has_conflicts: true,
        })
        .unwrap();
        assert_eq!(*sm.current_state(), SyncState::Resolving);

        sm.transition(SyncTrigger::AllConflictsResolved).unwrap();
        assert_eq!(*sm.current_state(), SyncState::Synced);

        sm.transition(SyncTrigger::ReportCompleted).unwrap();
        assert_eq!(*sm.current_state(), SyncState::Idle);
    }

    #[test]
    fn test_no_changes_path() {
        let mut sm = create_machine();

        sm.transition(SyncTrigger::TriggerSync).unwrap();
        sm.transition(SyncTrigger::PullCompleted).unwrap();
        sm.transition(SyncTrigger::DetectCompleted {
            has_changes: false,
            has_conflicts: false,
        })
        .unwrap();
        assert_eq!(*sm.current_state(), SyncState::Synced);

        sm.transition(SyncTrigger::ReportCompleted).unwrap();
        assert_eq!(*sm.current_state(), SyncState::Idle);
    }

    #[test]
    fn test_error_path_with_retry() {
        let mut sm = create_machine();

        sm.transition(SyncTrigger::TriggerSync).unwrap();
        sm.transition(SyncTrigger::OtherError).unwrap();
        assert_eq!(*sm.current_state(), SyncState::Error);
        assert_eq!(sm.attempt(), 1);
        assert!(sm.last_error().is_some());

        sm.transition(SyncTrigger::BackoffExpired).unwrap();
        assert_eq!(*sm.current_state(), SyncState::Pulling);
        assert_eq!(sm.attempt(), 1);

        sm.transition(SyncTrigger::PullCompleted).unwrap();
        assert_eq!(*sm.current_state(), SyncState::Detecting);
    }

    #[test]
    fn test_max_retries_exceeded() {
        let mut sm = SyncStateMachine::new(2);

        sm.transition(SyncTrigger::TriggerSync).unwrap();
        sm.transition(SyncTrigger::OtherError).unwrap();
        assert_eq!(sm.attempt(), 1);

        sm.transition(SyncTrigger::BackoffExpired).unwrap();
        sm.transition(SyncTrigger::OtherError).unwrap();
        assert_eq!(sm.attempt(), 2);

        sm.transition(SyncTrigger::BackoffExpired).unwrap();
        sm.transition(SyncTrigger::OtherError).unwrap();
        assert_eq!(sm.attempt(), 3);

        sm.transition(SyncTrigger::MaxRetriesExceeded).unwrap();
        assert_eq!(*sm.current_state(), SyncState::Idle);
        assert!(sm.last_error().is_none());
    }

    #[test]
    fn test_attempt_counter_increments_only_on_error() {
        let mut sm = create_machine();

        sm.transition(SyncTrigger::TriggerSync).unwrap();
        assert_eq!(sm.attempt(), 0);

        sm.transition(SyncTrigger::NetworkError).unwrap();
        assert_eq!(sm.attempt(), 0);

        sm.transition(SyncTrigger::BackoffExpired).unwrap();
        assert_eq!(sm.attempt(), 0);

        sm.transition(SyncTrigger::OtherError).unwrap();
        assert_eq!(sm.attempt(), 1);

        sm.transition(SyncTrigger::BackoffExpired).unwrap();
        assert_eq!(sm.attempt(), 1);
    }

    #[test]
    fn test_offline_path() {
        let mut sm = create_machine();

        sm.transition(SyncTrigger::TriggerSync).unwrap();
        sm.transition(SyncTrigger::NetworkError).unwrap();
        assert_eq!(*sm.current_state(), SyncState::Offline);
        assert_eq!(sm.attempt(), 0);

        sm.transition(SyncTrigger::BackoffExpired).unwrap();
        assert_eq!(*sm.current_state(), SyncState::Pulling);
    }

    #[test]
    fn test_offline_unlimited_retries() {
        let mut sm = create_machine();

        sm.transition(SyncTrigger::TriggerSync).unwrap();
        sm.transition(SyncTrigger::NetworkError).unwrap();
        assert_eq!(*sm.current_state(), SyncState::Offline);

        for _ in 1..=5 {
            sm.transition(SyncTrigger::BackoffExpired).unwrap();
            assert_eq!(*sm.current_state(), SyncState::Pulling);
            assert_eq!(sm.attempt(), 0);

            sm.transition(SyncTrigger::NetworkError).unwrap();
            assert_eq!(*sm.current_state(), SyncState::Offline);
        }
    }

    #[test]
    fn test_shutdown_from_idle() {
        let mut sm = create_machine();
        sm.transition(SyncTrigger::Shutdown).unwrap();
        assert_eq!(*sm.current_state(), SyncState::ShuttingDown);
    }

    #[test]
    fn test_shutdown_from_pulling() {
        let mut sm = create_machine();
        sm.transition(SyncTrigger::TriggerSync).unwrap();
        sm.transition(SyncTrigger::Shutdown).unwrap();
        assert_eq!(*sm.current_state(), SyncState::ShuttingDown);
    }

    #[test]
    fn test_shutdown_from_detecting() {
        let mut sm = create_machine();
        sm.transition(SyncTrigger::TriggerSync).unwrap();
        sm.transition(SyncTrigger::PullCompleted).unwrap();
        sm.transition(SyncTrigger::Shutdown).unwrap();
        assert_eq!(*sm.current_state(), SyncState::ShuttingDown);
    }

    #[test]
    fn test_shutdown_from_pushing() {
        let mut sm = create_machine();
        sm.transition(SyncTrigger::TriggerSync).unwrap();
        sm.transition(SyncTrigger::PullCompleted).unwrap();
        sm.transition(SyncTrigger::DetectCompleted {
            has_changes: true,
            has_conflicts: false,
        })
        .unwrap();
        sm.transition(SyncTrigger::Shutdown).unwrap();
        assert_eq!(*sm.current_state(), SyncState::ShuttingDown);
    }

    #[test]
    fn test_shutdown_from_resolving() {
        let mut sm = create_machine();
        sm.transition(SyncTrigger::TriggerSync).unwrap();
        sm.transition(SyncTrigger::PullCompleted).unwrap();
        sm.transition(SyncTrigger::DetectCompleted {
            has_changes: true,
            has_conflicts: true,
        })
        .unwrap();
        sm.transition(SyncTrigger::PushCompleted {
            has_conflicts: true,
        })
        .unwrap();
        sm.transition(SyncTrigger::Shutdown).unwrap();
        assert_eq!(*sm.current_state(), SyncState::ShuttingDown);
    }

    #[test]
    fn test_shutdown_from_error() {
        let mut sm = create_machine();
        sm.transition(SyncTrigger::TriggerSync).unwrap();
        sm.transition(SyncTrigger::OtherError).unwrap();
        sm.transition(SyncTrigger::Shutdown).unwrap();
        assert_eq!(*sm.current_state(), SyncState::ShuttingDown);
    }

    #[test]
    fn test_shutdown_from_offline() {
        let mut sm = create_machine();
        sm.transition(SyncTrigger::TriggerSync).unwrap();
        sm.transition(SyncTrigger::NetworkError).unwrap();
        sm.transition(SyncTrigger::Shutdown).unwrap();
        assert_eq!(*sm.current_state(), SyncState::ShuttingDown);
    }

    #[test]
    fn test_invalid_transition_pulling_to_idle() {
        let mut sm = create_machine();
        sm.transition(SyncTrigger::TriggerSync).unwrap();

        let result = sm.transition(SyncTrigger::ReportCompleted);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SyncError::InvalidStateTransition { .. }
        ));
    }

    #[test]
    fn test_invalid_transition_idle_to_detecting() {
        let mut sm = create_machine();
        let result = sm.transition(SyncTrigger::PullCompleted);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SyncError::InvalidStateTransition { .. }
        ));
    }

    #[test]
    fn test_invalid_transition_error_to_pulling_without_backoff() {
        let mut sm = create_machine();
        sm.transition(SyncTrigger::TriggerSync).unwrap();
        sm.transition(SyncTrigger::OtherError).unwrap();

        let result = sm.transition(SyncTrigger::TriggerSync);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_transition_synced_requires_report_completed() {
        let mut sm = create_machine();
        sm.transition(SyncTrigger::TriggerSync).unwrap();
        sm.transition(SyncTrigger::PullCompleted).unwrap();
        sm.transition(SyncTrigger::DetectCompleted {
            has_changes: false,
            has_conflicts: false,
        })
        .unwrap();

        let result = sm.transition(SyncTrigger::TriggerSync);
        assert!(result.is_err());
    }

    #[test]
    fn test_resolving_conflict_resolution_failed_stays_in_resolving() {
        let mut sm = create_machine();
        sm.transition(SyncTrigger::TriggerSync).unwrap();
        sm.transition(SyncTrigger::PullCompleted).unwrap();
        sm.transition(SyncTrigger::DetectCompleted {
            has_changes: true,
            has_conflicts: true,
        })
        .unwrap();
        sm.transition(SyncTrigger::PushCompleted {
            has_conflicts: true,
        })
        .unwrap();

        let result = sm.transition(SyncTrigger::ConflictResolutionFailed);
        assert!(result.is_ok());
        assert_eq!(*sm.current_state(), SyncState::Resolving);
    }

    #[test]
    fn test_resolving_network_error_goes_offline() {
        let mut sm = create_machine();
        sm.transition(SyncTrigger::TriggerSync).unwrap();
        sm.transition(SyncTrigger::PullCompleted).unwrap();
        sm.transition(SyncTrigger::DetectCompleted {
            has_changes: true,
            has_conflicts: true,
        })
        .unwrap();
        sm.transition(SyncTrigger::PushCompleted {
            has_conflicts: true,
        })
        .unwrap();

        let result = sm.transition(SyncTrigger::NetworkError);
        assert!(result.is_ok());
        assert_eq!(*sm.current_state(), SyncState::Offline);
    }

    #[test]
    fn test_reset_clears_attempt_and_error() {
        let mut sm = create_machine();

        sm.transition(SyncTrigger::TriggerSync).unwrap();
        sm.transition(SyncTrigger::OtherError).unwrap();
        assert_eq!(sm.attempt(), 1);
        assert!(sm.last_error().is_some());

        sm.reset();
        assert_eq!(*sm.current_state(), SyncState::Idle);
        assert_eq!(sm.attempt(), 0);
        assert!(sm.last_error().is_none());
    }

    #[test]
    fn test_reset_from_offline() {
        let mut sm = create_machine();

        sm.transition(SyncTrigger::TriggerSync).unwrap();
        sm.transition(SyncTrigger::NetworkError).unwrap();
        assert_eq!(*sm.current_state(), SyncState::Offline);

        sm.reset();
        assert_eq!(*sm.current_state(), SyncState::Idle);
        assert_eq!(sm.attempt(), 0);
    }

    #[test]
    fn test_can_accept_commands() {
        let mut sm = create_machine();

        assert!(sm.can_accept_commands());

        sm.transition(SyncTrigger::TriggerSync).unwrap();
        assert!(!sm.can_accept_commands());

        sm.transition(SyncTrigger::PullCompleted).unwrap();
        assert!(!sm.can_accept_commands());

        sm.transition(SyncTrigger::DetectCompleted {
            has_changes: true,
            has_conflicts: false,
        })
        .unwrap();
        assert!(!sm.can_accept_commands());

        sm.transition(SyncTrigger::PushCompleted {
            has_conflicts: false,
        })
        .unwrap();
        sm.transition(SyncTrigger::ReportCompleted).unwrap();
        sm.transition(SyncTrigger::TriggerSync).unwrap();
        sm.transition(SyncTrigger::PullCompleted).unwrap();
        sm.transition(SyncTrigger::DetectCompleted {
            has_changes: true,
            has_conflicts: true,
        })
        .unwrap();
        sm.transition(SyncTrigger::PushCompleted {
            has_conflicts: true,
        })
        .unwrap();
        assert!(!sm.can_accept_commands());

        sm.transition(SyncTrigger::AllConflictsResolved).unwrap();
        sm.transition(SyncTrigger::ReportCompleted).unwrap();
        assert!(sm.can_accept_commands());

        sm.transition(SyncTrigger::TriggerSync).unwrap();
        sm.transition(SyncTrigger::OtherError).unwrap();
        assert!(sm.can_accept_commands());

        sm.transition(SyncTrigger::MaxRetriesExceeded).unwrap();
        assert!(sm.can_accept_commands());

        sm.transition(SyncTrigger::TriggerSync).unwrap();
        sm.transition(SyncTrigger::NetworkError).unwrap();
        assert!(sm.can_accept_commands());

        sm.reset();
        assert!(sm.can_accept_commands());
    }

    #[test]
    fn test_synced_is_transient_detecting_no_changes() {
        let mut sm = create_machine();

        sm.transition(SyncTrigger::TriggerSync).unwrap();
        sm.transition(SyncTrigger::PullCompleted).unwrap();
        let result = sm
            .transition(SyncTrigger::DetectCompleted {
                has_changes: false,
                has_conflicts: false,
            })
            .unwrap();
        assert_eq!(result, SyncState::Synced);
        assert_eq!(*sm.current_state(), SyncState::Synced);

        sm.transition(SyncTrigger::ReportCompleted).unwrap();
        assert_eq!(*sm.current_state(), SyncState::Idle);
    }

    #[test]
    fn test_synced_is_transient_pushing_no_conflicts() {
        let mut sm = create_machine();

        sm.transition(SyncTrigger::TriggerSync).unwrap();
        sm.transition(SyncTrigger::PullCompleted).unwrap();
        sm.transition(SyncTrigger::DetectCompleted {
            has_changes: true,
            has_conflicts: false,
        })
        .unwrap();
        let result = sm
            .transition(SyncTrigger::PushCompleted {
                has_conflicts: false,
            })
            .unwrap();
        assert_eq!(result, SyncState::Synced);
        assert_eq!(*sm.current_state(), SyncState::Synced);

        sm.transition(SyncTrigger::ReportCompleted).unwrap();
        assert_eq!(*sm.current_state(), SyncState::Idle);
    }

    #[test]
    fn test_synced_is_transient_resolving_all_resolved() {
        let mut sm = create_machine();

        sm.transition(SyncTrigger::TriggerSync).unwrap();
        sm.transition(SyncTrigger::PullCompleted).unwrap();
        sm.transition(SyncTrigger::DetectCompleted {
            has_changes: true,
            has_conflicts: true,
        })
        .unwrap();
        sm.transition(SyncTrigger::PushCompleted {
            has_conflicts: true,
        })
        .unwrap();
        let result = sm.transition(SyncTrigger::AllConflictsResolved).unwrap();
        assert_eq!(result, SyncState::Synced);
        assert_eq!(*sm.current_state(), SyncState::Synced);

        sm.transition(SyncTrigger::ReportCompleted).unwrap();
        assert_eq!(*sm.current_state(), SyncState::Idle);
    }

    #[test]
    fn test_pull_only_trigger() {
        let mut sm = create_machine();

        let result = sm.transition(SyncTrigger::PullOnly);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SyncState::Pulling);
        assert_eq!(*sm.current_state(), SyncState::Pulling);
    }

    #[test]
    fn test_detect_completed_has_changes_true_no_conflicts() {
        let mut sm = create_machine();
        sm.transition(SyncTrigger::TriggerSync).unwrap();
        sm.transition(SyncTrigger::PullCompleted).unwrap();

        let result = sm.transition(SyncTrigger::DetectCompleted {
            has_changes: true,
            has_conflicts: false,
        });
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SyncState::Pushing);
    }

    #[test]
    fn test_detect_completed_has_conflicts_true_no_changes() {
        let mut sm = create_machine();
        sm.transition(SyncTrigger::TriggerSync).unwrap();
        sm.transition(SyncTrigger::PullCompleted).unwrap();

        let result = sm.transition(SyncTrigger::DetectCompleted {
            has_changes: false,
            has_conflicts: true,
        });
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SyncState::Pushing);
    }

    #[test]
    fn test_detect_completed_both_true() {
        let mut sm = create_machine();
        sm.transition(SyncTrigger::TriggerSync).unwrap();
        sm.transition(SyncTrigger::PullCompleted).unwrap();

        let result = sm.transition(SyncTrigger::DetectCompleted {
            has_changes: true,
            has_conflicts: true,
        });
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SyncState::Pushing);
    }
}
