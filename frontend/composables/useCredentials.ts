import { nextTick } from "vue";
import { invoke } from "../api/desktop";
import type {
  Entry,
  TrashEntry,
  Organization,
  GroupField,
} from "../types/models";
import type { VaultState } from "./state";
import type { VaultDependencies } from "./dependencies";
import { message } from "../utils/errors";
import { savedSecret } from "../utils/credentialForm";
export function useCredentials(
  state: Pick<
    VaultState,
    | "tagFilter"
    | "typeFilter"
    | "missingOnly"
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
    missingOnly,
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
    revealed.value = {};
  }
  const secretKey = (index: number, field: "password" | "token") =>
    `${index}:${field}`;
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
  async function reveal(index: number, field: "password" | "token") {
    const key = secretKey(index, field);
    if (key in revealed.value) {
      const next = { ...revealed.value };
      delete next[key];
      revealed.value = next;
      return;
    }
    const group = form.value.groups[index];
    if (!group) return;
    if (group[field]) {
      revealed.value = { ...revealed.value, [key]: group[field] };
      return;
    }
    const e = editing.value;
    if (!e || !savedSecret(group, field)) return;
    const epoch = revealEpoch.value;
    try {
      const secret = await invoke<string>("credentials.readSecret", {
        id: e.id,
        expectedRevision: e.revision,
        groupId: group.id,
        field,
      });
      if (
        epoch === revealEpoch.value &&
        editing.value?.id === e.id &&
        editing.value.revision === e.revision
      )
        revealed.value = { ...revealed.value, [key]: secret };
    } catch (value) {
      notify(message(value));
      await refresh();
    }
  }
  function updateGroupSecret(
    index: number,
    field: "password" | "token",
    value: string,
  ) {
    const key = secretKey(index, field);
    const visible = key in revealed.value;
    const group = form.value.groups[index];
    if (!group) return;
    group[field] = value;
    if (visible) revealed.value = { ...revealed.value, [key]: value };
  }
  function rejectForm(text: string) {
    formError.value = text;
    // 保存失败必须醒目：除了表单底部的红字，同时弹出顶部 toast 并滚动到错误区域。
    notify(text);
    void nextTick().then(() =>
      document
        .getElementById("form-error")
        ?.scrollIntoView({ block: "nearest" }),
    );
  }
  async function openUrl(url: string) {
    try {
      await invoke("browser.open", { url });
    } catch (value) {
      notify(message(value));
    }
  }
  async function copy(
    field: "url" | "host" | "port" | "database_name",
    serviceId?: string,
    index?: number,
  ) {
    try {
      const service =
        index !== undefined || serviceId
          ? form.value.services[
              index ?? form.value.services.findIndex((s) => s.id === serviceId)
            ]
          : undefined;
      const text =
        field === "host"
          ? form.value.database.host
          : field === "port"
            ? form.value.database.port
            : field === "database_name"
              ? form.value.database.databaseName
              : service
                ? service.url
                : form.value.url;
      if (!text) return;
      await invoke("clipboard.copyText", { text });
      notify("已复制");
    } catch (value) {
      notify(message(value));
    }
  }
  /** Copy a group field; saved secrets are copied by the backend, never read here. */
  async function copyGroup(index: number, field: GroupField) {
    const e = editing.value;
    const group = form.value.groups[index];
    if (!group) return;
    try {
      const text = group[field];
      if (text) await invoke("clipboard.copyText", { text });
      else if (field !== "account" && e && group.id)
        await invoke("clipboard.copyField", {
          id: e.id,
          expectedRevision: e.revision,
          field,
          groupId: group.id,
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
      const kind = formKind.value;
      const services =
        kind === "database"
          ? []
          : form.value.services
              .filter((service) => service.url.trim())
              .map((service) => ({ ...service, url: service.url.trim() }));
      if (kind === "api" && services.length === 0) {
        rejectForm("API 凭据必须填写至少一个网站 / API 地址。");
        return;
      }
      if (kind === "password" && !form.value.url.trim()) {
        rejectForm("账号密码必须填写归属网站。");
        return;
      }
      const groups = form.value.groups;
      const hasSecret = groups.some(
        (g) =>
          g.password ||
          g.token ||
          savedSecret(g, "password") ||
          savedSecret(g, "token"),
      );
      if ((kind === "password" || kind === "api") && !hasSecret) {
        rejectForm("至少需要一组填写了密码或 Token 的凭据。");
        return;
      }
      const payload = {
        tags: form.value.tags,
        notes: form.value.notes,
        documentationUrl: form.value.documentationUrl,
        database: kind === "database" ? form.value.database : null,
        services,
        url: kind === "api" ? (services[0]?.url ?? "") : form.value.url.trim(),
        // Entries are shown by their site; only generic entries keep a name.
        ...(kind === "generic" ? { title: form.value.title } : {}),
      };
      if (editing.value) {
        const kept = new Set(groups.map((g) => g.id).filter(Boolean));
        const removeGroups = editing.value.groups
          .map((g) => g.id)
          .filter((id) => !kept.has(id));
        // Saved groups send only changed secrets; an empty field keeps the stored one.
        const patch = {
          ...payload,
          id: editing.value.id,
          expectedRevision: editing.value.revision,
          groups: groups.map((g) => ({
            ...(g.id ? { id: g.id } : {}),
            environment: g.environment,
            account: g.account,
            ...(g.password || !g.id ? { password: g.password } : {}),
            ...(g.token || !g.id ? { token: g.token } : {}),
          })),
          ...(removeGroups.length ? { removeGroups } : {}),
        };
        updated = await invoke("credentials.update", patch);
      } else
        updated = await invoke("credentials.create", {
          ...payload,
          kind,
          groups: groups.map(({ environment, account, password, token }) => ({
            environment,
            account,
            password,
            token,
          })),
        });
      const key = draftKey();
      if (key) drafts.delete(key);
      switching.value = true;
      query.value = "";
      tagFilter.value = "";
      typeFilter.value = "";
      missingOnly.value = false;
      creating.value = false;
      await refresh();
      selected.value = updated.id;
      loadEntry(updated, false);
      switching.value = false;
      notify("已加密保存");
    } catch (value) {
      rejectForm(message(value));
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
    updateGroupSecret,
    openUrl,
    copy,
    copyGroup,
    save,
    askDelete,
    confirmDelete,
    restore,
    emptyTrash,
  };
}
