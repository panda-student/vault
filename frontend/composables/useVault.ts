import { onMounted, onUnmounted, watch } from "vue";
import { invoke } from "../api/desktop";
import type { Info } from "../types/models";
import { createVaultState } from "./state";
import type { VaultDependencies } from "./dependencies";
import { useUpdater } from "./useUpdates";
import { message } from "../utils/errors";
import { useCredentialDraft } from "./useCredentialDraft";
import { useCredentials } from "./useCredentials";
import { useTags } from "./useTags";
import { useSettings } from "./useSettings";
import { useNavigation } from "./useNavigation";
import { useTransfer } from "./useTransfer";
export function useVault() {
  const state = createVaultState();
  const dependencies: VaultDependencies = {
    notify: (value) => navigation.notify(value),
    hide: () => credentials.hide(),
    invalidateReveal: () => credentials.invalidateReveal(),
    refresh: () => credentials.refresh(),
    stashDraft: () => draft.stashDraft(),
    loadEntry: (...args) => draft.loadEntry(...args),
    draftKey: () => draft.draftKey(),
    reconcile: (entry) => draft.reconcile(entry),
    navigate: (page) => navigation.navigate(page),
  };
  const draft = useCredentialDraft(state, dependencies);
  const credentials = useCredentials(state, dependencies);
  const tags = useTags(state, dependencies);
  const settings = useSettings(state, dependencies);
  const navigation = useNavigation(state, dependencies);
  const {
    editing,
    creating,
    dirty,
    permissionDraft,
    saving,
    settingsSaving,
    drafts,
    info,
    loading,
    error,
    list,
    selected,
    switching,
    detail,
    form,
  } = state;
  const { hide, refresh, invalidateReveal } = credentials;
  const { stashDraft, loadEntry, reconcile } = draft;
  const transfer = useTransfer({ notify: dependencies.notify, refresh });
  const updater = useUpdater(
    () =>
      !!((editing.value || creating.value) && dirty.value) ||
      drafts.size > 0 ||
      !!permissionDraft.value ||
      !!transfer.pendingExport.value ||
      saving.value ||
      settingsSaving.value ||
      !!document.querySelector("dialog[open]:not(#update-dialog)"),
  );
  let timer: ReturnType<typeof setInterval> | undefined;
  watch(list, (values) => {
    if (!values.some((e) => e.id === selected.value))
      selected.value = values[0]?.id ?? null;
  });
  watch(
    selected,
    () => {
      if (!switching.value && !creating.value) {
        stashDraft();
        loadEntry(detail.value);
      }
    },
    { flush: "sync" },
  );
  watch(
    () => detail.value?.revision,
    () => {
      hide();
      invalidateReveal();
      if (!switching.value && !creating.value && detail.value)
        reconcile(detail.value);
    },
  );
  watch(
    form,
    () => {
      hide();
      invalidateReveal();
    },
    { deep: true, flush: "sync" },
  );

  function focus() {
    void refresh();
  }
  function visibility() {
    if (document.visibilityState !== "visible") hide();
  }
  onMounted(async () => {
    try {
      info.value = await invoke<Info>("application.info");
      await refresh();
      if (import.meta.env.MODE === "test" && info.value.smokeTest) {
        const { runSmoke } = await import("../../tests/support/frontend/smoke");
        await runSmoke();
      } else {
        updater.start();
        timer = setInterval(() => {
          if (document.visibilityState === "visible") void refresh();
        }, 2000);
        window.addEventListener("focus", focus);
        window.addEventListener("blur", hide);
        document.addEventListener("visibilitychange", visibility);
      }
    } catch (value) {
      error.value = message(value);
      loading.value = false;
    }
  });
  onUnmounted(() => {
    clearInterval(timer);
    window.removeEventListener("focus", focus);
    window.removeEventListener("blur", hide);
    document.removeEventListener("visibilitychange", visibility);
  });

  return {
    ...state,
    ...draft,
    ...credentials,
    ...tags,
    ...settings,
    ...navigation,
    ...transfer,
    updater,
  };
}
export type VaultUI = ReturnType<typeof useVault>;
