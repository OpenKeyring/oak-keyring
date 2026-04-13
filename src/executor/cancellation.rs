//! CancellationToken infrastructure for long operation cancellation.

use tokio_util::sync::CancellationToken;

/// Wrapper providing executor-specific utilities for CancellationToken.
pub struct ExecutorCancellationToken {
    inner: CancellationToken,
}

impl ExecutorCancellationToken {
    pub fn new(token: CancellationToken) -> Self {
        Self { inner: token }
    }

    /// Create a child token for a specific long-running operation.
    pub fn child_token(&self) -> CancellationToken {
        self.inner.child_token()
    }

    pub fn is_cancelled(&self) -> bool {
        self.inner.is_cancelled()
    }

    pub fn inner(&self) -> &CancellationToken {
        &self.inner
    }
}

impl std::ops::Deref for ExecutorCancellationToken {
    type Target = CancellationToken;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
