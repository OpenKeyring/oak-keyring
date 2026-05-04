/// Error severity levels for categorizing error conditions.
///
/// Each level represents a different category of error severity:
/// - **Fatal**: System-level errors that prevent the application from functioning
/// - **Operation**: User-actionable errors that require user intervention
/// - **Minor**: Temporary or non-critical errors that may resolve themselves
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorLevel {
    /// Fatal errors indicate system-level failures that prevent core functionality.
    ///
    /// These errors typically represent corrupted data, crypto failures, or
    /// identity mismatches that require administrator intervention or data recovery.
    Fatal,

    /// Operation errors are user-actionable issues that require user intervention.
    ///
    /// These include validation failures, conflicts, missing credentials, and
    /// other conditions where the user needs to take corrective action.
    Operation,

    /// Minor errors represent temporary or non-critical issues.
    ///
    /// These include network timeouts, rate limiting, transient failures, and
    /// other conditions that may resolve on retry or are non-blocking.
    Minor,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_levels_are_exhaustive() {
        // Ensure all error levels can be created and compared
        let fatal = ErrorLevel::Fatal;
        let operation = ErrorLevel::Operation;
        let minor = ErrorLevel::Minor;

        assert_eq!(fatal, ErrorLevel::Fatal);
        assert_eq!(operation, ErrorLevel::Operation);
        assert_eq!(minor, ErrorLevel::Minor);

        // Verify inequality
        assert_ne!(fatal, operation);
        assert_ne!(operation, minor);
        assert_ne!(fatal, minor);
    }

    #[test]
    fn error_levels_are_copy() {
        // Verify ErrorLevel implements Copy (can be copied without move)
        let level = ErrorLevel::Fatal;
        let copied = level; // This should work with Copy trait
        assert_eq!(level, copied);
    }

    #[test]
    fn error_levels_support_debug() {
        // Verify Debug implementation provides meaningful output
        let fatal = ErrorLevel::Fatal;
        let operation = ErrorLevel::Operation;
        let minor = ErrorLevel::Minor;

        assert_eq!(format!("{:?}", fatal), "Fatal");
        assert_eq!(format!("{:?}", operation), "Operation");
        assert_eq!(format!("{:?}", minor), "Minor");
    }
}
