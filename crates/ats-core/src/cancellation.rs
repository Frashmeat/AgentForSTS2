use std::sync::{Arc, Mutex};

use tokio::sync::Notify;

use crate::platform::domain::CancellationReason;

/// Cooperative cancellation shared by Core operations and Platform Runs.
///
/// The first reason is immutable so competing lifecycle sources cannot rewrite
/// the persisted cancellation provenance.
#[derive(Debug, Clone, Default)]
pub struct CancellationToken {
    inner: Arc<CancellationState>,
}

#[derive(Debug, Default)]
struct CancellationState {
    reason: Mutex<Option<CancellationReason>>,
    notify: Notify,
}

impl CancellationToken {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self, reason: CancellationReason) -> bool {
        let inserted = {
            let mut current = self
                .inner
                .reason
                .lock()
                .expect("cancellation lock poisoned");
            if current.is_some() {
                false
            } else {
                *current = Some(reason);
                true
            }
        };
        self.inner.notify.notify_waiters();
        inserted
    }

    #[must_use]
    pub fn reason(&self) -> Option<CancellationReason> {
        *self
            .inner
            .reason
            .lock()
            .expect("cancellation lock poisoned")
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.reason().is_some()
    }

    pub async fn cancelled(&self) -> CancellationReason {
        loop {
            let notified = self.inner.notify.notified();
            if let Some(reason) = self.reason() {
                return reason;
            }
            notified.await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn first_cancellation_reason_wins_and_wakes_waiters() {
        let token = CancellationToken::new();
        let waiter = {
            let token = token.clone();
            tokio::spawn(async move { token.cancelled().await })
        };

        assert!(token.cancel(CancellationReason::ProjectClose));
        assert!(!token.cancel(CancellationReason::AppShutdown));
        assert_eq!(waiter.await.unwrap(), CancellationReason::ProjectClose);
        assert_eq!(token.reason(), Some(CancellationReason::ProjectClose));
    }

    #[tokio::test]
    async fn cancelled_observes_a_reason_published_before_waiting() {
        let token = CancellationToken::new();
        token.cancel(CancellationReason::User);

        assert_eq!(token.cancelled().await, CancellationReason::User);
    }
}
