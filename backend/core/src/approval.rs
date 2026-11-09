//! One pending MCP request waiting for the on-screen desktop user.
//!
//! The MCP side blocks in `wait`; the desktop polls `pending`, claims the
//! request with `claim`, and answers with `finish`. A failed attempt can be
//! handed back with `release` so the user may retry within the same wait.
use std::{
    sync::{Condvar, Mutex},
    time::{Duration, Instant},
};
use vault_contracts::errors::{AppError, Result};

pub(crate) struct Approval<T, O> {
    state: Mutex<Option<Pending<T, O>>>,
    wake: Condvar,
}

struct Pending<T, O> {
    id: String,
    request: T,
    requested_at: i64,
    claimed: bool,
    outcome: Option<O>,
}

/// Errors a waiting caller can see; each kind of request words its own.
pub(crate) struct Messages {
    pub busy: AppError,
    pub ended: fn() -> AppError,
    pub timeout: AppError,
}

impl<T, O> Default for Approval<T, O> {
    fn default() -> Self {
        Self {
            state: Mutex::new(None),
            wake: Condvar::new(),
        }
    }
}

impl<T: Clone, O> Approval<T, O> {
    pub fn begin(&self, request: T, busy: AppError) -> Result<String> {
        let mut state = self.state.lock().map_err(|_| AppError::storage())?;
        if state.is_some() {
            return Err(busy);
        }
        let id = uuid::Uuid::new_v4().to_string();
        *state = Some(Pending {
            id: id.clone(),
            request,
            requested_at: crate::context::now(),
            claimed: false,
            outcome: None,
        });
        Ok(id)
    }

    /// The request the desktop should show, if one is waiting unanswered.
    pub fn pending(&self) -> Result<Option<(String, T, i64)>> {
        let state = self.state.lock().map_err(|_| AppError::storage())?;
        Ok(state.as_ref().and_then(|p| {
            (!p.claimed && p.outcome.is_none())
                .then(|| (p.id.clone(), p.request.clone(), p.requested_at))
        }))
    }

    /// Take the request for answering; `None` if it is gone or already taken.
    pub fn claim(&self, id: &str) -> Result<Option<T>> {
        let mut state = self.state.lock().map_err(|_| AppError::storage())?;
        Ok(match state.as_mut() {
            Some(p) if p.id == id && !p.claimed && p.outcome.is_none() => {
                p.claimed = true;
                Some(p.request.clone())
            }
            _ => None,
        })
    }

    /// Let the desktop answer again, e.g. after a mistyped passphrase.
    pub fn release(&self, id: &str) {
        if let Ok(mut state) = self.state.lock()
            && let Some(p) = state.as_mut()
            && p.id == id
        {
            p.claimed = false;
        }
    }

    pub fn finish(&self, id: &str, outcome: O) {
        if let Ok(mut state) = self.state.lock() {
            if let Some(p) = state.as_mut()
                && p.id == id
            {
                p.outcome = Some(outcome);
            }
            self.wake.notify_all();
        }
    }

    pub fn wait(&self, id: &str, timeout: Duration, messages: Messages) -> Result<O> {
        let deadline = Instant::now() + timeout;
        let mut state = self.state.lock().map_err(|_| AppError::storage())?;
        loop {
            match state.as_mut() {
                Some(p) if p.id == id => {
                    if let Some(outcome) = p.outcome.take() {
                        *state = None;
                        return Ok(outcome);
                    }
                }
                _ => return Err((messages.ended)()),
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            // A claimed request is being answered; give the desktop time to finish.
            if remaining.is_zero() && !state.as_ref().is_some_and(|p| p.claimed) {
                *state = None;
                return Err(messages.timeout);
            }
            let (next, _) = self
                .wake
                .wait_timeout(state, remaining.max(Duration::from_millis(200)))
                .map_err(|_| AppError::storage())?;
            state = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn messages() -> Messages {
        Messages {
            busy: AppError::new("BUSY", "busy"),
            ended: || AppError::new("ENDED", "ended"),
            timeout: AppError::new("TIMEOUT", "timeout"),
        }
    }

    #[test]
    fn one_request_at_a_time_and_released_requests_reappear() {
        let approval = Approval::<u8, bool>::default();
        let id = approval.begin(3, AppError::new("BUSY", "busy")).unwrap();
        assert_eq!(
            approval
                .begin(1, AppError::new("BUSY", "busy"))
                .unwrap_err()
                .code,
            "BUSY"
        );
        assert_eq!(approval.claim(&id).unwrap(), Some(3));
        assert!(approval.claim(&id).unwrap().is_none());
        assert!(approval.pending().unwrap().is_none());
        approval.release(&id);
        assert_eq!(approval.pending().unwrap().unwrap().1, 3);
        approval.finish(&id, true);
        assert!(
            approval
                .wait(&id, Duration::from_secs(1), messages())
                .unwrap()
        );
        assert!(approval.pending().unwrap().is_none());
    }

    #[test]
    fn unanswered_request_times_out_and_clears() {
        let approval = Approval::<u8, bool>::default();
        let id = approval.begin(1, AppError::new("BUSY", "busy")).unwrap();
        let error = approval
            .wait(&id, Duration::from_millis(10), messages())
            .unwrap_err();
        assert_eq!(error.code, "TIMEOUT");
        assert!(approval.pending().unwrap().is_none());
    }
}
