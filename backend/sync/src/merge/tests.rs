use super::*;
use vault_contracts::credentials::{CredentialGroup, EntryInput, Kind};

fn rec(id: &str, password: &str, updated_at: i64, device: &str) -> SyncRecord {
    SyncRecord {
        id: id.into(),
        value: EntryInput {
            kind: Kind::Password,
            title: format!("条目 {id}"),
            tags: vec![],
            database: None,
            url: "https://example.com".into(),
            documentation_url: String::new(),
            services: vec![],
            notes: String::new(),
            groups: vec![CredentialGroup {
                id: "g".into(),
                account: "me".into(),
                password: password.into(),
                ..Default::default()
            }],
        },
        pinned: false,
        created_at: 1,
        updated_at,
        deleted_at: None,
        device: device.into(),
    }
}
fn base_of(records: &[SyncRecord]) -> Base {
    records
        .iter()
        .map(|r| (r.id.clone(), content_hash(r)))
        .collect()
}
fn run(
    local: &[SyncRecord],
    remote: Option<&[SyncRecord]>,
    base: Option<&Base>,
    choice: Option<SyncChoice>,
) -> Merged {
    merge(
        &Input {
            local,
            remote,
            base,
            now: 1_700_000_000_000,
        },
        choice,
    )
}
fn upserts(m: &Merged) -> Vec<&SyncRecord> {
    m.local
        .iter()
        .filter_map(|c| match c {
            LocalChange::Upsert(r) => Some(r),
            LocalChange::Purge(_) => None,
        })
        .collect()
}
fn purges(m: &Merged) -> Vec<&str> {
    m.local
        .iter()
        .filter_map(|c| match c {
            LocalChange::Purge(id) => Some(id.as_str()),
            LocalChange::Upsert(_) => None,
        })
        .collect()
}
fn remote_ids(m: &Merged) -> Vec<&str> {
    m.remote.iter().map(|r| r.id.as_str()).collect()
}
fn password(r: &SyncRecord) -> &str {
    &r.value.groups[0].password
}

#[test]
fn identical_sides_write_nothing() {
    let both = [rec("a", "p", 1, "x")];
    let m = run(&both, Some(&both), Some(&base_of(&both)), None);
    assert!(!m.write_remote);
    assert!(m.local.is_empty());
    assert_eq!(m.outcome, SyncOutcome::default());
}

#[test]
fn new_entries_flow_both_ways() {
    let base = base_of(&[rec("old", "p", 1, "x")]);
    let local = [rec("old", "p", 1, "x"), rec("mine", "p", 2, "本机")];
    let remote = [rec("old", "p", 1, "x"), rec("theirs", "p", 3, "笔记本")];
    let m = run(&local, Some(&remote), Some(&base), None);
    assert_eq!(remote_ids(&m), ["mine", "old", "theirs"]);
    assert_eq!(upserts(&m).len(), 1);
    assert_eq!(upserts(&m)[0].id, "theirs");
    assert!(m.write_remote);
    assert_eq!((m.outcome.pushed, m.outcome.pulled), (1, 1));
    assert_eq!(m.base.len(), 3);
}

#[test]
fn one_sided_edits_win_without_asking() {
    let start = [rec("a", "v1", 1, "x"), rec("b", "v1", 1, "x")];
    let base = base_of(&start);
    let local = [rec("a", "local-v2", 5, "本机"), rec("b", "v1", 1, "x")];
    let remote = [rec("a", "v1", 1, "x"), rec("b", "remote-v2", 6, "笔记本")];
    let m = run(&local, Some(&remote), Some(&base), None);
    assert!(m.conflicts.is_empty());
    let final_a = m.remote.iter().find(|r| r.id == "a").unwrap();
    assert_eq!(password(final_a), "local-v2");
    assert_eq!(password(upserts(&m)[0]), "remote-v2");
}

#[test]
fn trash_and_pin_changes_are_changes() {
    let start = [rec("a", "p", 1, "x"), rec("b", "p", 1, "x")];
    let base = base_of(&start);
    let mut trashed = rec("a", "p", 1, "本机");
    trashed.deleted_at = Some(9);
    let mut pinned = rec("b", "p", 1, "笔记本");
    pinned.pinned = true;
    let m = run(
        &[trashed, rec("b", "p", 1, "x")],
        Some(&[rec("a", "p", 1, "x"), pinned]),
        Some(&base),
        None,
    );
    assert!(
        m.remote
            .iter()
            .find(|r| r.id == "a")
            .unwrap()
            .deleted_at
            .is_some()
    );
    assert!(upserts(&m)[0].pinned);
}

#[test]
fn permanent_deletion_spreads_unless_the_other_side_edited() {
    let start = [rec("gone", "p", 1, "x"), rec("edited", "p", 1, "x")];
    let base = base_of(&start);
    // The remote purged both; this computer only edited "edited" since.
    let local = [rec("gone", "p", 1, "x"), rec("edited", "new", 4, "本机")];
    let m = run(&local, Some(&[]), Some(&base), None);
    assert_eq!(purges(&m), ["gone"]);
    assert_eq!(remote_ids(&m), ["edited"]);
}

#[test]
fn both_sides_edited_waits_for_the_user() {
    let start = [rec("a", "v1", 1, "x")];
    let base = base_of(&start);
    let local = [rec("a", "local", 5, "本机")];
    let remote = [rec("a", "remote", 7, "笔记本")];
    let m = run(&local, Some(&remote), Some(&base), None);
    assert_eq!(m.conflicts.len(), 1);
    let c = &m.conflicts[0];
    assert_eq!(
        (c.local_changed_at, c.remote_changed_at),
        (Some(5), Some(7))
    );
    assert_eq!(
        (c.local_device.as_str(), c.remote_device.as_str()),
        ("本机", "笔记本")
    );
    // Neither side changes, and the base still remembers the old agreement.
    assert!(m.local.is_empty());
    assert_eq!(password(&m.remote[0]), "remote");
    assert!(!m.write_remote);
    assert_eq!(m.base["a"], base["a"]);
}

#[test]
fn conflict_choices_pick_a_side_or_keep_both() {
    let base = base_of(&[rec("a", "v1", 1, "x")]);
    let local = [rec("a", "local", 5, "本机")];
    let remote = [rec("a", "remote", 7, "笔记本")];

    let m = run(
        &local,
        Some(&remote),
        Some(&base),
        Some(SyncChoice::LocalWins),
    );
    assert_eq!(password(&m.remote[0]), "local");
    assert!(m.local.is_empty());

    let m = run(
        &local,
        Some(&remote),
        Some(&base),
        Some(SyncChoice::RemoteWins),
    );
    assert_eq!(password(upserts(&m)[0]), "remote");

    let m = run(&local, Some(&remote), Some(&base), Some(SyncChoice::TwoWay));
    assert_eq!(m.remote.len(), 2);
    let kept = m.remote.iter().find(|r| r.id == "a").unwrap();
    assert_eq!(password(kept), "remote", "newer version keeps the id");
    let copy = m.remote.iter().find(|r| r.id != "a").unwrap();
    assert_eq!(password(copy), "local");
    assert!(copy.value.title.contains("冲突副本"));
    assert!(copy.value.title.contains("本机"));
    // Locally: the kept version replaces "a" and the copy is added.
    assert_eq!(upserts(&m).len(), 2);
    assert_eq!(m.base.len(), 2);
}

#[test]
fn first_join_with_data_on_both_sides_asks_first() {
    let local = [rec("shared", "same", 1, "x"), rec("mine", "p", 2, "本机")];
    let remote = [
        rec("shared", "same", 1, "x"),
        rec("theirs", "p", 3, "笔记本"),
    ];
    let m = run(&local, Some(&remote), None, None);
    assert!(m.first_join);
    assert!(!m.write_remote && m.local.is_empty() && m.base.is_empty());
    assert_eq!(m.conflicts.len(), 2);

    let m = run(&local, Some(&remote), None, Some(SyncChoice::LocalWins));
    assert_eq!(remote_ids(&m), ["mine", "shared"]);
    assert!(m.local.is_empty());

    // Remote wins: local-only entries go to the trash, never deleted.
    let m = run(&local, Some(&remote), None, Some(SyncChoice::RemoteWins));
    let mine = m.remote.iter().find(|r| r.id == "mine").unwrap();
    assert!(mine.deleted_at.is_some());
    assert!(purges(&m).is_empty());
    assert!(upserts(&m).iter().any(|r| r.id == "theirs"));

    let m = run(&local, Some(&remote), None, Some(SyncChoice::TwoWay));
    assert_eq!(remote_ids(&m), ["mine", "shared", "theirs"]);
}

#[test]
fn first_join_without_conflicting_data_needs_no_choice() {
    let remote = [rec("a", "p", 1, "笔记本")];
    // Empty local vault: restore everything.
    let m = run(&[], Some(&remote), None, None);
    assert!(!m.first_join);
    assert_eq!(upserts(&m).len(), 1);
    // Both sides already identical.
    let m = run(&remote, Some(&remote), None, None);
    assert!(!m.first_join && m.local.is_empty());
    assert_eq!(m.base.len(), 1);
}

#[test]
fn a_missing_remote_file_is_refilled_not_treated_as_deletion() {
    let local = [rec("a", "p", 1, "x")];
    let base = base_of(&local);
    let m = run(&local, None, Some(&base), None);
    assert!(purges(&m).is_empty());
    assert!(m.write_remote);
    assert_eq!(remote_ids(&m), ["a"]);
}

#[test]
fn conflict_copy_dates_are_readable() {
    assert_eq!(utc_minute(0), "1970-01-01 00:00 UTC");
    assert_eq!(utc_minute(1_762_441_200_000), "2025-11-06 15:00 UTC");
}
