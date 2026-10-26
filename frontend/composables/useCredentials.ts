import { nextTick } from "vue";
import { invoke } from "../api/desktop";
import type { Entry, TrashEntry, Organization } from "../types/models";
import type { VaultState } from "./state";
import type { VaultDependencies } from "./dependencies";
import { message } from "../utils/errors";
export function useCredentials(
  state: Pick<
    VaultState,
    | "tagFilter"
    | "typeFilter"
    | "organization"
    | "trash"
    | "query"
    | "entries"
    | "selected"
    | "revealed"
    | "revealEpoch"
    | "loading"
    | "error"
    | "saving"
    | "deleteDialog"
    | "editing"
    | "pendingDelete"
    | "formKind"
    | "formError"
    | "form"
    | "creating"
    | "dirty"
    | "drafts"
    | "switching"
    | "detail"
  >,
  dependencies: Pick<VaultDependencies, "notify" | "loadEntry" | "draftKey">,
) {
  const {
    tagFilter,
    typeFilter,
    organization,
    trash,
    query,
    entries,
    selected,
    revealed,
    revealEpoch,
    loading,
    error,

    saving,
    deleteDialog,
    editing,
    pendingDelete,
    formKind,
    formError,
    form,
    creating,
    dirty,
    drafts,
    switching,
    detail,
  } = state;
  const { notify, loadEntry, draftKey } = dependencies;
  let fetching = false;
  function invalidateReveal() {
    revealEpoch.value++;
  }
  function hide() {
    revealed.value = null;
  }
  async function refresh() {
    if (fetching) return;
    fetching = true;
    try {
      const [items, org, deleted] = await Promise.all([
        invoke<Entry[]>("credentials.list"),
        invoke<Organization>("tags.list"),
        invoke<TrashEntry[]>("trash.list"),
      ]);
      entries.value = items;
      organization.value = org;
      trash.value = deleted;
      error.value = "";
    } catch (value) {
      error.value = message(value);
    } finally {
      fetching = false;
      loading.value = false;
    }
  }
  async function pin(entry: Entry) {
    try {
      await invoke("credentials.pin", {
        id: entry.id,
        expectedRevision: entry.revision,
        pinned: !entry.pinned,
      });
      await refresh();
    } catch (value) {
      notify(message(value));
      await refresh();
    }
  }
  async function reveal() {
    if (revealed.value !== null) {
      hide();
      return;
    }
    if (form.value.secret) {
      revealed.value = form.value.secret;
      return;
    }
    const e = editing.value;
    if (!e) return;
    const epoch = revealEpoch.value;
    try {
      const secret = await invoke<string>("credentials.readSecret", {
        id: e.id,
        expectedRevision: e.revision,
      });
      if (
        epoch === revealEpoch.value &&
        editing.value?.id === e.id &&
        editing.value.revision === e.revision
      )
        revealed.value = secret;
    } catch (value) {
      notify(message(value));
      await refresh();
    }
  }
  function updateSecret(value: string) {
    const visible = revealed.value !== null;
    form.value.secret = value;
    if (visible) revealed.value = value;
  }
  async function openUrl(url: string) {
    try {
      await invoke("browser.open", { url });
    } catch (value) {
      notify(message(value));
    }
  }
  async function copy(
    field: "account" | "secret" | "url" | "host" | "port" | "database_name",
    serviceId?: string,
    index?: number,
  ) {
    const e = editing.value;
    try {
      const text =
        field === "host"
          ? form.value.database.host
          : field === "port"
            ? form.value.database.port
            : field === "database_name"
              ? form.value.database.databaseName
              : field === "account"
                ? form.value.account
                : field === "url"
                  ? formKind.value === "api"
                    ? (form.value.services[
                        index ??
                          form.value.services.findIndex(
                            (s) => s.id === serviceId,
                          )
                      ]?.url ?? "")
                    : form.value.url
                  : form.value.secret;
      if (field !== "secret" || text)
        await invoke("clipboard.copyText", { text });
      else if (e)
        await invoke("clipboard.copyField", {
          id: e.id,
          expectedRevision: e.revision,
          field,
        });
      else return;
      notify("已复制");
    } catch (value) {
      notify(message(value));
    }
  }
  async function save() {
    if (saving.value || (editing.value && !dirty.value)) return;
    saving.value = true;
    formError.value = "";
    try {
      let updated: Entry;
      const services =
        formKind.value === "api"
          ? form.value.services
              .filter((service) => service.url.trim())
              .map((service) => ({ ...service, url: service.url.trim() }))
          : [];
      const payload = {
        ...form.value,
        database: formKind.value === "database" ? form.value.database : null,
        services,
        url:
          formKind.value === "api" ? (services[0]?.url ?? "") : form.value.url,
      };
      if (editing.value) {
        // 非 API 条目不携带 services：后端要求 services 与 url 一致，传空数组会与之冲突。
        const { secret, services: entryServices, ...fields } = payload;
        const patch: Record<string, unknown> = {
          ...fields,
          id: editing.value.id,
          expectedRevision: editing.value.revision,
          ...(secret ? { secret } : {}),
        };
        if (formKind.value === "api") patch.services = entryServices;
        updated = await invoke("credentials.update", patch);
      } else
        updated = await invoke("credentials.create", {
          ...payload,
          kind: formKind.value,
        });
      const key = draftKey();
      if (key) drafts.delete(key);
      switching.value = true;
      query.value = "";
      tagFilter.value = "";
      typeFilter.value = "";
      creating.value = false;
      await refresh();
      selected.value = updated.id;
      loadEntry(updated, false);
      switching.value = false;
      notify("已加密保存");
    } catch (value) {
      formError.value = message(value);
      // 保存失败必须醒目：除了表单底部的红字，同时弹出顶部 toast 并滚动到错误区域。
      notify(formError.value);
      void nextTick().then(() =>
        document
          .getElementById("form-error")
          ?.scrollIntoView({ block: "nearest" }),
      );
    } finally {
      switching.value = false;
      saving.value = false;
    }
  }
  function askDelete() {
    if (!detail.value) return;
    hide();
    pendingDelete.value = { ...detail.value };
    deleteDialog.value?.showModal();
  }
  async function confirmDelete() {
    if (!pendingDelete.value || saving.value) return;
    saving.value = true;
    try {
      drafts.delete(pendingDelete.value.id);
      await invoke("credentials.delete", {
        id: pendingDelete.value.id,
        expectedRevision: pendingDelete.value.revision,
      });
      deleteDialog.value?.close();
      pendingDelete.value = null;
      await refresh();
      notify("已移入垃圾桶，可恢复");
    } catch (value) {
      deleteDialog.value?.close();
      notify(message(value));
      await refresh();
    } finally {
      saving.value = false;
    }
  }
  async function restore(entry: TrashEntry) {
    if (saving.value) return;
    saving.value = true;
    try {
      await invoke("trash.restore", {
        id: entry.id,
        expectedRevision: entry.revision,
      });
      await refresh();
      notify("已恢复，标签已保留");
    } catch (v) {
      notify(message(v));
    } finally {
      saving.value = false;
    }
  }
  async function emptyTrash() {
    if (saving.value) return;
    saving.value = true;
    try {
      const count = await invoke<number>("trash.empty");
      await refresh();
      if (count) notify(`已永久删除 ${count} 条凭据`);
    } catch (v) {
      notify(message(v));
    } finally {
      saving.value = false;
    }
  }
  return {
    invalidateReveal,
    hide,
    refresh,
    pin,
    reveal,
    updateSecret,
    openUrl,
    copy,
    save,
    askDelete,
    confirmDelete,
    restore,
    emptyTrash,
  };
}
