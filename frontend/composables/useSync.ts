import { computed, onMounted, onUnmounted, ref } from "vue";
import { invoke, pickSyncFolder } from "../api/desktop";
import type {
  PendingSync,
  SyncChoice,
  SyncCredential,
  SyncSetup,
  SyncStatus,
} from "../types/sync";
import type { VaultDependencies } from "./dependencies";
import { message } from "../utils/errors";

const POLL_MS = 2000;

/** Conflicts with the same entries and kind were already put off by the user. */
function conflictKey(status: SyncStatus | null) {
  if (!status?.conflicts.length) return "";
  return `${status.firstJoin}:${status.conflicts.map((c) => c.id).join(",")}`;
}

export function useSync(
  dependencies: Pick<VaultDependencies, "notify" | "refresh">,
) {
  const { notify, refresh } = dependencies;
  const syncStatus = ref<SyncStatus | null>(null);
  const syncCredentials = ref<SyncCredential[]>([]);
  const syncBusy = ref(false);
  const pendingSync = ref<PendingSync | null>(null);
  const pendingSyncBusy = ref(false);
  const postponedConflicts = ref("");
  let polling: ReturnType<typeof setInterval> | undefined;
  let pollingNow = false;

  const conflictsWaiting = computed(() => {
    const key = conflictKey(syncStatus.value);
    return key !== "" && key !== postponedConflicts.value;
  });

  /** A finished sync may have changed entries; show them. */
  function apply(status: SyncStatus) {
    const previous = syncStatus.value?.lastSyncAt;
    syncStatus.value = status;
    if (status.lastSyncAt && status.lastSyncAt !== previous) void refresh();
  }

  /** Bumped when an action starts, so an older poll reply cannot undo its result. */
  let generation = 0;
  function begin() {
    if (syncBusy.value) return false;
    syncBusy.value = true;
    generation += 1;
    return true;
  }
  async function loadSyncStatus() {
    const started = generation;
    const status = await invoke<SyncStatus>("sync.status");
    if (started === generation && !syncBusy.value) apply(status);
  }
  async function loadSyncCredentials() {
    syncCredentials.value = await invoke<SyncCredential[]>("sync.credentials");
  }
  async function pollPending() {
    if (pendingSyncBusy.value) return;
    const { pending } = await invoke<{ pending: PendingSync | null }>(
      "sync.pending",
    );
    if (!pendingSyncBusy.value && pendingSync.value?.id !== pending?.id)
      pendingSync.value = pending;
  }
  async function poll() {
    if (pollingNow) return;
    pollingNow = true;
    // An AI request must reach the user even while the window is hidden or in
    // the tray; the status only matters while it is shown.
    await pollPending().catch(() => undefined);
    if (document.visibilityState === "visible" && !syncBusy.value)
      await loadSyncStatus().catch(() => undefined);
    pollingNow = false;
  }

  /** Run a sync action; the status it returns replaces the shown one. */
  async function act(method: "sync.run" | "sync.disable", success: string) {
    if (!begin()) return false;
    try {
      apply(await invoke<SyncStatus>(method));
      notify(success);
      return true;
    } catch (value) {
      notify(message(value));
      return false;
    } finally {
      syncBusy.value = false;
    }
  }
  /** Errors go to the caller's form; a saved target shows in the status. */
  async function configureSync(setup: SyncSetup) {
    if (!begin()) return null;
    try {
      const status = await invoke<SyncStatus>("sync.configure", setup);
      apply(status);
      return status;
    } finally {
      syncBusy.value = false;
    }
  }
  async function testSync(setup: SyncSetup) {
    return invoke<{ remoteExists: boolean }>("sync.test", setup);
  }
  async function resolveSync(choice: SyncChoice) {
    if (!begin()) return false;
    try {
      apply(await invoke<SyncStatus>("sync.resolve", { choice }));
      notify("已按你的选择完成同步");
      return true;
    } catch (value) {
      notify(message(value));
      return false;
    } finally {
      syncBusy.value = false;
    }
  }
  function postponeConflicts() {
    postponedConflicts.value = conflictKey(syncStatus.value);
  }
  function reviewConflicts() {
    postponedConflicts.value = "";
  }
  const runSync = () => act("sync.run", "同步完成");
  const disableSync = () =>
    act("sync.disable", "已停用同步，本机和远端的数据都没有删除");

  /** Answer the AI's request. A wrong passphrase keeps the request open. */
  async function respondSync(passphrase: string | null) {
    const request = pendingSync.value;
    if (!request || pendingSyncBusy.value) return;
    pendingSyncBusy.value = true;
    generation += 1;
    try {
      const cancel = passphrase === null;
      const result = await invoke<SyncStatus | { status: string }>(
        "sync.respond",
        { id: request.id, passphrase, cancel },
      );
      pendingSync.value = null;
      if ("configured" in result) apply(result);
      notify(cancel ? "已拒绝 AI 的同步设置" : "同步位置已设置");
    } finally {
      pendingSyncBusy.value = false;
    }
  }

  onMounted(() => {
    void poll();
    polling = setInterval(() => void poll(), POLL_MS);
  });
  onUnmounted(() => clearInterval(polling));

  return {
    syncStatus,
    syncCredentials,
    syncBusy,
    pendingSync,
    pendingSyncBusy,
    conflictsWaiting,
    loadSyncCredentials,
    pickSyncFolder,
    configureSync,
    testSync,
    runSync,
    resolveSync,
    postponeConflicts,
    reviewConflicts,
    disableSync,
    respondSync,
  };
}
