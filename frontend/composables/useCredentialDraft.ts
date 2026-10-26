import { cloneJson } from "../utils/json";
import { invoke } from "../api/desktop";
import type { Entry } from "../types/models";
import type { VaultState } from "./state";
import type { VaultDependencies } from "./dependencies";
import { fieldsFor } from "../utils/credentialForm";
import { message } from "../utils/errors";
export function useCredentialDraft(
  state: Pick<
    VaultState,
    | "tagFilter"
    | "typeFilter"
    | "entries"
    | "selected"
    | "saving"
    | "editing"
    | "formKind"
    | "formError"
    | "form"
    | "creating"
    | "baseline"
    | "dirty"
    | "drafts"
    | "switching"
    | "detail"
  >,
  dependencies: Pick<VaultDependencies, "notify" | "hide" | "invalidateReveal">,
) {
  const {
    tagFilter,
    typeFilter,
    entries,
    selected,
    saving,
    editing,
    formKind,
    formError,
    form,
    creating,
    baseline,
    dirty,
    drafts,
    switching,
    detail,
  } = state;
  const { notify, hide, invalidateReveal } = dependencies;

  function draftKey() {
    return creating.value ? `new:${formKind.value}` : editing.value?.id;
  }
  function stashDraft() {
    const key = draftKey();
    if (!key) return;
    if (dirty.value)
      drafts.set(key, {
        entry: editing.value,
        kind: formKind.value,
        fields: cloneJson(form.value),
        baseline: baseline.value,
      });
    else drafts.delete(key);
  }
  function loadEntry(entry?: Entry, restore = true) {
    hide();
    invalidateReveal();
    formError.value = "";
    creating.value = false;
    const draft = entry && restore ? drafts.get(entry.id) : undefined;
    editing.value = draft?.entry ?? (entry ? { ...entry } : null);
    formKind.value = entry?.kind ?? "password";
    form.value = draft?.fields ?? fieldsFor(entry);
    baseline.value = draft?.baseline ?? JSON.stringify(form.value);
    if (
      entry &&
      draft?.entry?.revision !== undefined &&
      draft.entry.revision !== entry.revision
    )
      reconcile(entry);
  }
  function reconcile(entry: Entry) {
    if (
      !editing.value ||
      editing.value.id !== entry.id ||
      editing.value.revision === entry.revision
    )
      return;
    if (!dirty.value) {
      loadEntry(entry, false);
      return;
    }
    if (
      JSON.stringify(fieldsFor(entry)) ===
      JSON.stringify(fieldsFor(editing.value))
    ) {
      editing.value = { ...entry };
    } else
      formError.value =
        "此条目已被其他操作修改。你的输入已保留，请撤销修改以载入最新内容；旧版本不会覆盖新版本。";
  }
  function selectEntry(id: string) {
    if (saving.value) return;
    stashDraft();
    switching.value = true;
    creating.value = false;
    selected.value = id;
    loadEntry(entries.value.find((entry) => entry.id === id));
    switching.value = false;
  }
  function openForm() {
    if (saving.value) return;
    stashDraft();
    hide();
    invalidateReveal();
    formError.value = "";
    creating.value = true;
    editing.value = null;
    formKind.value = typeFilter.value || "password";
    const draft = drafts.get(`new:${formKind.value}`);
    form.value = draft?.fields ?? {
      ...fieldsFor(),
      tags: tagFilter.value ? [tagFilter.value] : [],
    };
    baseline.value = draft?.baseline ?? JSON.stringify(form.value);
  }
  function addService() {
    if (form.value.services.length < 20)
      form.value.services.push({ id: "", protocol: "openai", url: "" });
  }
  function closeForm() {
    if (saving.value) return;
    const key = draftKey();
    if (key) drafts.delete(key);
    loadEntry(detail.value, false);
  }
  async function duplicate(sameSite = false) {
    if (saving.value || !editing.value) return;
    const source = editing.value;
    const fields = cloneJson(form.value);
    const kind = formKind.value;
    try {
      let secret = "";
      if (!sameSite)
        secret =
          fields.secret ||
          (await invoke<string>("credentials.readSecret", {
            id: source.id,
            expectedRevision: source.revision,
          }));
      openForm();
      formKind.value = kind;
      form.value = sameSite
        ? { ...fieldsFor(), url: fields.url, services: [] }
        : { ...fields, title: fields.title + " · 副本", secret };
      if (sameSite) form.value.tags = [...fields.tags];
      baseline.value = "";
      notify(
        sameSite
          ? "已保留网站与标签，账号密码为空"
          : "已复制为草稿，请修改环境和凭据",
      );
    } catch (v) {
      notify(message(v));
    }
  }
  return {
    draftKey,
    stashDraft,
    loadEntry,
    reconcile,
    selectEntry,
    openForm,
    addService,
    closeForm,
    duplicate,
  };
}
