//! Background syncing in the process that holds the vault: shortly after
//! start, every configured interval, and soon after any entry changes.
use crate::context::CoreSession;
use std::{
    sync::{Arc, Condvar, Mutex, PoisonError},
    time::{Duration, Instant},
};
use vault_contracts::errors::{AppError, Result};

const FIRST_DELAY: Duration = Duration::from_secs(5);
/// Edits within this window go out together.
const DEBOUNCE: Duration = Duration::from_secs(30);
/// Wait before retrying edits whose sync failed (network down, remote busy).
const RETRY: Duration = Duration::from_secs(5 * 60);
/// How often the worker rechecks the configuration and interval.
const TICK: Duration = Duration::from_secs(60);

#[derive(Default)]
pub(crate) struct Schedule {
    dirty_at: Mutex<Option<Instant>>,
    wake: Condvar,
}
impl Schedule {
    pub(crate) fn mark_dirty(&self) {
        self.due_after(DEBOUNCE);
    }

    fn retry_later(&self) {
        self.due_after(RETRY);
    }

    fn due_after(&self, delay: Duration) {
        let mut dirty = self.dirty_at.lock().unwrap_or_else(PoisonError::into_inner);
        dirty.get_or_insert_with(|| Instant::now() + delay);
        self.wake.notify_all();
    }

    /// Wait up to `timeout`; true when a debounced edit is due.
    fn wait(&self, timeout: Duration) -> bool {
        let end = Instant::now() + timeout;
        let mut dirty = self.dirty_at.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            let now = Instant::now();
            if dirty.is_some_and(|due| now >= due) {
                *dirty = None;
                return true;
            }
            if now >= end {
                return false;
            }
            let until = dirty.map_or(end, |due| due.min(end));
            dirty = self
                .wake
                .wait_timeout(dirty, until - now)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }
}

impl CoreSession {
    /// Start the background sync thread. Bootstrap calls this once, only in
    /// the process that owns the vault.
    pub fn start_sync_worker(self: &Arc<Self>) -> Result<()> {
        let session = self.clone();
        std::thread::Builder::new()
            .name("vault-sync".into())
            .spawn(move || session.sync_loop())
            .map(|_| ())
            .map_err(|_| AppError::storage())
    }

    fn sync_loop(&self) {
        let sync = &self.service.sync;
        let mut first = true;
        loop {
            let edited = sync.schedule.wait(if first { FIRST_DELAY } else { TICK });
            if self.service.updater.installing() {
                return;
            }
            let Ok(Some(config)) = self.sync_config() else {
                first = false;
                continue;
            };
            let interval = Duration::from_secs(u64::from(config.interval_minutes) * 60);
            let last = sync.last_run.lock().ok().and_then(|last| *last);
            let due = config.interval_minutes > 0 && last.is_none_or(|t| t.elapsed() >= interval);
            // The outcome and any error are kept in the sync status. A failed
            // run after edits is tried again later, even with no interval set.
            if (first || edited || due) && self.sync_run(None).is_err() && edited {
                sync.schedule.retry_later();
            }
            first = false;
        }
    }
}
