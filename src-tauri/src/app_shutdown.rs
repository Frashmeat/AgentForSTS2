use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use ats_runtime::CancellationReason;

use crate::project_session::{ActiveProject, DrainTimeout};

const EXIT_IDLE: u8 = 0;
const EXIT_DRAINING: u8 = 1;
const EXIT_ALLOWED: u8 = 2;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum ExitRequestAction {
    StartDrain,
    Prevent,
    Allow,
}

pub(crate) struct AppShutdown {
    phase: AtomicU8,
}

impl AppShutdown {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            phase: AtomicU8::new(EXIT_IDLE),
        }
    }

    pub(crate) fn on_exit_requested(&self) -> ExitRequestAction {
        loop {
            match self.phase.load(Ordering::Acquire) {
                EXIT_ALLOWED => return ExitRequestAction::Allow,
                EXIT_DRAINING => return ExitRequestAction::Prevent,
                EXIT_IDLE => {
                    if self
                        .phase
                        .compare_exchange(
                            EXIT_IDLE,
                            EXIT_DRAINING,
                            Ordering::AcqRel,
                            Ordering::Acquire,
                        )
                        .is_ok()
                    {
                        return ExitRequestAction::StartDrain;
                    }
                }
                _ => return ExitRequestAction::Prevent,
            }
        }
    }

    pub(crate) fn allow_exit(&self) {
        self.phase.store(EXIT_ALLOWED, Ordering::Release);
    }

    pub(crate) fn retry_after_failure(&self) {
        let _ = self.phase.compare_exchange(
            EXIT_DRAINING,
            EXIT_IDLE,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }
}

impl Default for AppShutdown {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub(crate) enum ShutdownDrainError {
    ActiveProjectPoisoned,
    ProjectLockPoisoned,
    Timeout(DrainTimeout),
}

impl std::fmt::Display for ShutdownDrainError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ActiveProjectPoisoned => formatter.write_str("active project state is poisoned"),
            Self::ProjectLockPoisoned => formatter.write_str("project lock state is poisoned"),
            Self::Timeout(timeout) => write!(
                formatter,
                "project runs did not stop before timeout; first blocking run={}",
                timeout
                    .blocked_runs
                    .first()
                    .map_or("<unknown>", |run_id| run_id.as_str())
            ),
        }
    }
}

pub(crate) async fn drain_active_project(
    active: &ActiveProject,
    timeout: Duration,
) -> Result<(), ShutdownDrainError> {
    let _lifecycle = active.lifecycle.lock().await;
    let Some(session) = active
        .current()
        .map_err(|_| ShutdownDrainError::ActiveProjectPoisoned)?
    else {
        return Ok(());
    };
    session
        .cancel_and_drain(CancellationReason::AppShutdown, timeout)
        .await
        .map_err(ShutdownDrainError::Timeout)?;
    session
        .release_project_lock()
        .map_err(|_| ShutdownDrainError::ProjectLockPoisoned)?;
    active
        .replace(None)
        .map_err(|_| ShutdownDrainError::ActiveProjectPoisoned)?;
    drop(session);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_state_prevents_reentry_until_internal_exit_is_allowed() {
        let shutdown = AppShutdown::new();
        assert_eq!(shutdown.on_exit_requested(), ExitRequestAction::StartDrain);
        assert_eq!(shutdown.on_exit_requested(), ExitRequestAction::Prevent);
        shutdown.allow_exit();
        assert_eq!(shutdown.on_exit_requested(), ExitRequestAction::Allow);
    }

    #[test]
    fn failed_drain_can_be_retried() {
        let shutdown = AppShutdown::new();
        assert_eq!(shutdown.on_exit_requested(), ExitRequestAction::StartDrain);
        shutdown.retry_after_failure();
        assert_eq!(shutdown.on_exit_requested(), ExitRequestAction::StartDrain);
    }
}
