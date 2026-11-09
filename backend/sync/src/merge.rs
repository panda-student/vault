//! Three-way merge of local records, remote records and the last synced base.
//!
//! Changes are detected by content hash, not timestamps, so clocks that differ
//! between computers cannot pick the wrong side. Timestamps only tell the user
//! which version is newer, and order the two versions in a two-way resolution.
use std::collections::{BTreeMap, BTreeSet};
pub use vault_contracts::sync::LocalChange;
use vault_contracts::sync::{SyncChoice, SyncConflict, SyncOutcome, SyncRecord};

/// Record id → content hash as of the last successful sync.
pub type Base = BTreeMap<String, String>;

pub struct Input<'a> {
    pub local: &'a [SyncRecord],
    /// `None` when the remote has no sync file yet.
    pub remote: Option<&'a [SyncRecord]>,
    /// `None` when this computer has never synced with this remote.
    pub base: Option<&'a Base>,
    pub now: i64,
}

#[derive(Debug)]
pub struct Merged {
    /// Complete content to write to the remote.
    pub remote: Vec<SyncRecord>,
    /// Whether the remote must be written at all.
    pub write_remote: bool,
    pub local: Vec<LocalChange>,
    /// Base to save after both sides are written.
    pub base: Base,
    /// Entries waiting for the user's choice; neither side changes them.
    pub conflicts: Vec<SyncConflict>,
    /// First join with data on both sides: nothing is written until the user chooses.
    pub first_join: bool,
    pub outcome: SyncOutcome,
}

pub fn content_hash(record: &SyncRecord) -> String {
    let canonical =
        serde_json::to_vec(&(&record.value, record.pinned, record.deleted_at.is_some()))
            .expect("records serialize");
    crate::sha_hex(&canonical)
}

/// A record with its content hash, computed once.
#[derive(Clone)]
struct Hashed {
    record: SyncRecord,
    hash: String,
}
impl Hashed {
    fn new(record: SyncRecord) -> Self {
        let hash = content_hash(&record);
        Self { record, hash }
    }
}

fn index(records: &[SyncRecord]) -> BTreeMap<&str, Hashed> {
    records
        .iter()
        .map(|record| (record.id.as_str(), Hashed::new(record.clone())))
        .collect()
}

/// What one id becomes after the merge.
enum Resolution {
    Keep(Hashed),
    /// Keep the first under its id and add the second as a conflict copy.
    KeepWithCopy(Hashed, Hashed),
    Drop,
    /// Both sides changed and no choice was given: the remote keeps its version.
    Pending(SyncConflict, Hashed),
}

struct Context {
    first_join: bool,
    choice: Option<SyncChoice>,
    now: i64,
}

/// Merge without a choice leaves conflicts pending; with a choice it settles them.
pub fn merge(input: &Input, choice: Option<SyncChoice>) -> Merged {
    let local = index(input.local);
    let remote = index(input.remote.unwrap_or_default());
    let empty = Base::new();
    // A missing remote file means "push everything", never "the remote deleted all".
    let base = match input.remote {
        Some(_) => input.base.unwrap_or(&empty),
        None => &empty,
    };
    let first_join = input.base.is_none() && !local.is_empty() && !remote.is_empty();
    if first_join && choice.is_none() {
        let differing = first_join_differences(&local, &remote);
        if !differing.is_empty() {
            return awaiting_first_join(input, differing);
        }
    }

    let context = Context {
        first_join,
        choice,
        now: input.now,
    };
    let ids: BTreeSet<&str> = local
        .keys()
        .chain(remote.keys())
        .copied()
        .chain(base.keys().map(String::as_str))
        .collect();
    let mut result: BTreeMap<String, Hashed> = BTreeMap::new();
    let mut pending: Vec<SyncConflict> = vec![];
    for id in ids {
        match resolve(id, local.get(id), remote.get(id), base.get(id), &context) {
            Resolution::Keep(record) => {
                result.insert(id.into(), record);
            }
            Resolution::KeepWithCopy(kept, copy) => {
                result.insert(id.into(), kept);
                result.insert(copy.record.id.clone(), copy);
            }
            Resolution::Pending(conflict, remote_version) => {
                pending.push(conflict);
                result.insert(id.into(), remote_version);
            }
            Resolution::Drop => {}
        }
    }

    let pending_ids: BTreeSet<&str> = pending.iter().map(|c| c.id.as_str()).collect();
    let changes = local_changes(&local, &result, &pending_ids);
    let new_base = next_base(&result, base, &pending_ids);
    let (outcome, write_remote) = tally(&local, &remote, &result, &changes, input.remote.is_none());
    Merged {
        remote: result.into_values().map(|h| h.record).collect(),
        write_remote,
        local: changes,
        base: new_base,
        outcome: SyncOutcome {
            conflicts: pending.len(),
            ..outcome
        },
        conflicts: pending,
        first_join: false,
    }
}

fn first_join_differences(
    local: &BTreeMap<&str, Hashed>,
    remote: &BTreeMap<&str, Hashed>,
) -> Vec<SyncConflict> {
    let ids: BTreeSet<&str> = local.keys().chain(remote.keys()).copied().collect();
    ids.into_iter()
        .filter(|id| local.get(id).map(|h| &h.hash) != remote.get(id).map(|h| &h.hash))
        .map(|id| {
            conflict(
                id,
                local.get(id).map(|h| &h.record),
                remote.get(id).map(|h| &h.record),
            )
        })
        .collect()
}

/// First join with differing data on both sides: write nothing, ask the user.
fn awaiting_first_join(input: &Input, differing: Vec<SyncConflict>) -> Merged {
    Merged {
        remote: input.remote.unwrap_or_default().to_vec(),
        write_remote: false,
        local: vec![],
        base: Base::new(),
        outcome: SyncOutcome {
            conflicts: differing.len(),
            ..Default::default()
        },
        conflicts: differing,
        first_join: true,
    }
}

/// Decide one id from its local, remote and base versions.
fn resolve(
    id: &str,
    local: Option<&Hashed>,
    remote: Option<&Hashed>,
    base: Option<&String>,
    context: &Context,
) -> Resolution {
    match (local, remote) {
        (Some(l), Some(r)) => {
            if l.hash == r.hash {
                Resolution::Keep(r.clone())
            } else if base == Some(&r.hash) {
                Resolution::Keep(l.clone())
            } else if base == Some(&l.hash) {
                Resolution::Keep(r.clone())
            } else {
                // Both sides differ from what was last agreed.
                both_changed(id, l, r, context)
            }
        }
        (Some(l), None) => match (context.first_join, context.choice, base) {
            // Remote-wins first join keeps local-only entries, in the trash.
            (true, Some(SyncChoice::RemoteWins), _) => {
                let mut trashed = l.record.clone();
                trashed.deleted_at.get_or_insert(context.now);
                Resolution::Keep(Hashed::new(trashed))
            }
            // Removed on the remote; drop it here too unless changed here since.
            (_, _, Some(b)) if *b == l.hash => Resolution::Drop,
            _ => Resolution::Keep(l.clone()),
        },
        (None, Some(r)) => match (context.first_join, context.choice, base) {
            (true, Some(SyncChoice::LocalWins), _) => Resolution::Drop,
            (_, _, Some(b)) if *b == r.hash => Resolution::Drop,
            _ => Resolution::Keep(r.clone()),
        },
        (None, None) => Resolution::Drop,
    }
}

fn both_changed(id: &str, l: &Hashed, r: &Hashed, context: &Context) -> Resolution {
    match context.choice {
        None => Resolution::Pending(conflict(id, Some(&l.record), Some(&r.record)), r.clone()),
        Some(SyncChoice::LocalWins) => Resolution::Keep(l.clone()),
        Some(SyncChoice::RemoteWins) => Resolution::Keep(r.clone()),
        Some(SyncChoice::TwoWay) => {
            let (newer, older) = if r.record.changed_at() > l.record.changed_at() {
                (r, l)
            } else {
                (l, r)
            };
            Resolution::KeepWithCopy(
                newer.clone(),
                Hashed::new(conflict_copy(&older.record, context.now)),
            )
        }
    }
}

/// Local writes that make this computer match the merged result.
fn local_changes(
    local: &BTreeMap<&str, Hashed>,
    result: &BTreeMap<String, Hashed>,
    pending: &BTreeSet<&str>,
) -> Vec<LocalChange> {
    let mut changes: Vec<LocalChange> = result
        .iter()
        .filter(|(id, merged)| {
            !pending.contains(id.as_str())
                && local.get(id.as_str()).map(|h| &h.hash) != Some(&merged.hash)
        })
        .map(|(_, merged)| LocalChange::Upsert(merged.record.clone()))
        .collect();
    changes.extend(
        local
            .keys()
            .filter(|id| !result.contains_key(**id))
            .map(|id| LocalChange::Purge((*id).into())),
    );
    changes
}

/// Agreed hashes after this sync; pending conflicts keep their old agreement.
fn next_base(result: &BTreeMap<String, Hashed>, old: &Base, pending: &BTreeSet<&str>) -> Base {
    result
        .iter()
        .filter_map(|(id, merged)| {
            if pending.contains(id.as_str()) {
                old.get(id).map(|hash| (id.clone(), hash.clone()))
            } else {
                Some((id.clone(), merged.hash.clone()))
            }
        })
        .collect()
}

/// Counts for the user, and whether the remote must be written.
fn tally(
    local: &BTreeMap<&str, Hashed>,
    remote: &BTreeMap<&str, Hashed>,
    result: &BTreeMap<String, Hashed>,
    changes: &[LocalChange],
    remote_missing: bool,
) -> (SyncOutcome, bool) {
    let pushed = result
        .iter()
        .filter(|(id, merged)| remote.get(id.as_str()).map(|h| &h.hash) != Some(&merged.hash))
        .count();
    let removed_remotely = remote.keys().filter(|id| !result.contains_key(**id));
    let removed_locally = local.keys().filter(|id| !result.contains_key(**id));
    let deleted: BTreeSet<&str> = removed_remotely.chain(removed_locally).copied().collect();
    let remote_shrank = remote.keys().any(|id| !result.contains_key(*id));
    let pulled = changes
        .iter()
        .filter(|c| matches!(c, LocalChange::Upsert(_)))
        .count();
    (
        SyncOutcome {
            pushed,
            pulled,
            deleted: deleted.len(),
            conflicts: 0,
        },
        remote_missing || pushed > 0 || remote_shrank,
    )
}

fn conflict(id: &str, local: Option<&SyncRecord>, remote: Option<&SyncRecord>) -> SyncConflict {
    let name = local
        .or(remote)
        .map(|record| record.value.display_name())
        .unwrap_or_default();
    SyncConflict {
        id: id.into(),
        name,
        local_changed_at: local.map(SyncRecord::changed_at),
        remote_changed_at: remote.map(SyncRecord::changed_at),
        local_device: local.map(|r| r.device.clone()).unwrap_or_default(),
        remote_device: remote.map(|r| r.device.clone()).unwrap_or_default(),
    }
}

/// The losing version of a two-way resolution, kept as a new entry.
fn conflict_copy(older: &SyncRecord, now: i64) -> SyncRecord {
    let mut copy = older.clone();
    copy.id = uuid::Uuid::new_v4().to_string();
    copy.value.title = format!(
        "{}（冲突副本 · {} · {}）",
        older.value.display_name(),
        if older.device.is_empty() {
            "未知设备"
        } else {
            &older.device
        },
        utc_minute(older.changed_at())
    );
    copy.created_at = now;
    copy.updated_at = now;
    copy
}

/// `YYYY-MM-DD HH:MM UTC` for a millisecond timestamp.
fn utc_minute(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let minutes = secs.rem_euclid(86_400) / 60;
    // Civil-from-days (Howard Hinnant), valid for the proleptic Gregorian calendar.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        minutes / 60,
        minutes % 60
    )
}

#[cfg(test)]
mod tests;
