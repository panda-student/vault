import { cloneJson } from "../utils/json";
import { invoke } from "../api/desktop";
import type { Entry } from "../types/models";
import type { VaultState } from "./state";
import type { VaultDependencies } from "./dependencies";
import {
  emptyGroup,
  emptyService,
  fieldsFor,
  savedSecret,
} from "../utils/credentialForm";
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
      form.value.services.push(emptyService());
  }
  function addGroup() {
    if (form.value.groups.length < 50) form.value.groups.push(emptyGroup());
  }
  function removeGroup(index: number) {
    // Revealed values are keyed by group position, which shifts here.
    hide();
    form.value.groups.splice(index, 1);
  }
  function closeForm() {
    if (saving.value) return;
    const key = draftKey();
    if (key) drafts.delete(key);
    loadEntry(detail.value, false);
  }
  async function duplicate() {
    if (saving.value || !editing.value) return;
    const source = editing.value;
    const fields = cloneJson(form.value);
    const kind = formKind.value;
    try {
      // The copy is a new entry, so saved secrets are read into its draft once.
      for (const group of fields.groups) {
        for (const field of ["password", "token"] as const) {
          if (!group[field] && savedSecret(group, field))
            group[field] = await invoke<string>("credentials.readSecret", {
              id: source.id,
              expectedRevision: source.revision,
              groupId: group.id,
              field,
            });
        }
        group.id = "";
        group.hasPassword = false;
        group.hasToken = false;
      }
      openForm();
      formKind.value = kind;
      form.value = {
        ...fields,
        title:
          kind === "generic" && fields.title ? fields.title + " · 副本" : "",
      };
      baseline.value = "";
      notify("已复制为草稿，请修改后保存");
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
    addGroup,
    removeGroup,
    closeForm,
    duplicate,
  };
}
