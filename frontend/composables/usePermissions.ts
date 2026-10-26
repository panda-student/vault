import { cloneJson } from "../utils/json";
import { computed, ref, watch, onUnmounted } from "vue";
import type { UnwrapRef } from "vue";
import { invoke } from "../api/desktop";
import type { VaultUI } from "../composables/useVault";
import type { Settings, PermissionKey } from "../types/models";
export function usePermissions(v: UnwrapRef<VaultUI>) {
  const props = { v };

  const draft = ref<Settings | null>(
    props.v.permissionDraft
      ? cloneJson(props.v.permissionDraft)
      : props.v.info
        ? cloneJson(props.v.info.settings)
        : null,
  );
  const open = ref(false),
    expanded = ref(new Set<string>()),
    query = ref(""),
    previewId = ref(""),
    previewDialog = ref<HTMLDialogElement | null>(null),
    previewError = ref("");
  const columns: { key: PermissionKey; label: string }[] = [
    { key: "access", label: "允许访问" },
    { key: "readMetadata", label: "读取信息" },
    { key: "create", label: "新增" },
    { key: "update", label: "修改" },
    { key: "delete", label: "移入垃圾桶" },
    { key: "fillInput", label: "窗口填写" },
    { key: "writeFile", label: "写入文件" },
  ];
  const policy = (s: Settings | null | undefined) =>
    s
      ? JSON.stringify([
          s.mcpPermissions,
          s.mcpAccess,
          s.mcpTagRules,
          s.mcpEntryRules,
        ])
      : "";
  const dirty = computed(
    () => policy(draft.value) !== policy(props.v.info?.settings),
  );
  watch(
    draft,
    (s) => {
      props.v.permissionDraft = s && dirty.value ? cloneJson(s) : null;
    },
    { deep: true },
  );
  watch(
    () => props.v.info?.settings,
    (s) => {
      if (!s) return;
      if (!props.v.permissionDraft) draft.value = cloneJson(s);
      else if (draft.value) {
        draft.value.mcpEnabled = s.mcpEnabled;
        draft.value.closeToTray = s.closeToTray;
      }
    },
    { deep: true },
  );
  type Effective = { configured: boolean; allowed: boolean; source: string };
  type Projection = {
    id: string;
    permissions: Record<PermissionKey, Effective>;
  };
  const projections = ref<Record<string, Projection["permissions"]>>({});
  let epoch = 0,
    timer: ReturnType<typeof setTimeout> | undefined;
  watch(
    [draft, () => props.v.entries],
    () => {
      clearTimeout(timer);
      const current = ++epoch;
      timer = setTimeout(async () => {
        if (!draft.value) return;
        try {
          const rows = await invoke<Projection[]>("permissions.preview", {
            settings: cloneJson(draft.value),
          });
          if (current !== epoch) return;
          projections.value = Object.fromEntries(
            rows.map((e) => [e.id, e.permissions]),
          );
          previewError.value = "";
        } catch {
          if (current === epoch)
            previewError.value = "无法计算生效权限，请重试。";
        }
      }, 80);
    },
    { deep: true, immediate: true },
  );
  onUnmounted(() => {
    clearTimeout(timer);
    epoch++;
  });
  type Row = {
    kind: "global" | "tag" | "entry" | "untagged";
    id: string;
    parent: string;
    title: string;
    note: string;
  };
  const rows = computed(() => {
    const result: Row[] = [
      {
        kind: "global",
        id: "global",
        parent: "",
        title: "默认规则",
        note: "适用于未单独设置的操作",
      },
    ];
    if (!open.value) return result;
    const q = query.value.trim().toLowerCase();
    for (const tag of props.v.organization.tags) {
      const related = props.v.entries.filter((e) => e.tags.includes(tag));
      const match = related.filter(
        (e) =>
          !q ||
          tag.toLowerCase().includes(q) ||
          [e.title, e.account, ...e.tags].join(" ").toLowerCase().includes(q),
      );
      if (q && !match.length && !tag.toLowerCase().includes(q)) continue;
      result.push({
        kind: "tag",
        id: tag,
        parent: "",
        title: tag,
        note: related.length + " 条凭据",
      });
      if (expanded.value.has(tag) || q)
        for (const e of match)
          result.push({
            kind: "entry",
            id: e.id,
            parent: tag,
            title: e.title,
            note: e.tags.join(" · "),
          });
    }
    const untagged = props.v.entries.filter(
      (e) =>
        !e.tags.length &&
        (!q || [e.title, e.account].join(" ").toLowerCase().includes(q)),
    );
    if (untagged.length) {
      result.push({
        kind: "untagged",
        id: "untagged",
        parent: "",
        title: "无标签凭据",
        note: "",
      });
      for (const e of untagged)
        result.push({
          kind: "entry",
          id: e.id,
          parent: "untagged",
          title: e.title,
          note: "继承默认规则",
        });
    }
    return result;
  });
  function own(row: Row, key: PermissionKey) {
    if (!draft.value) return false;
    return (
      row.kind === "global" ||
      Object.hasOwn(
        row.kind === "tag"
          ? draft.value.mcpTagRules[row.id] || {}
          : draft.value.mcpEntryRules[row.id] || {},
        key,
      )
    );
  }
  function hasRule(row: Row) {
    return (
      draft.value &&
      row.kind !== "global" &&
      Object.keys(
        row.kind === "tag"
          ? draft.value.mcpTagRules[row.id] || {}
          : draft.value.mcpEntryRules[row.id] || {},
      ).length > 0
    );
  }
  function defaultValue(key: PermissionKey) {
    return draft.value
      ? key === "access"
        ? draft.value.mcpAccess
        : draft.value.mcpPermissions[key]
      : false;
  }
  function value(row: Row, key: PermissionKey): boolean {
    if (!draft.value) return false;
    if (row.kind === "global") return defaultValue(key);
    if (row.kind === "tag")
      return draft.value.mcpTagRules[row.id]?.[key] ?? defaultValue(key);
    return projections.value[row.id]?.[key]?.configured ?? false;
  }
  function source(row: Row, key: PermissionKey) {
    return row.kind === "entry"
      ? projections.value[row.id]?.[key]?.source || "正在计算"
      : own(row, key)
        ? row.kind === "global"
          ? "默认规则"
          : "标签设置"
        : "继承默认规则";
  }
  function set(row: Row, key: PermissionKey, event: Event) {
    if (!draft.value) return;
    const checked = (event.target as HTMLInputElement).checked;
    if (row.kind === "global") {
      if (key === "access") draft.value.mcpAccess = checked;
      else draft.value.mcpPermissions[key] = checked;
    } else {
      const map =
        row.kind === "tag"
          ? draft.value.mcpTagRules
          : draft.value.mcpEntryRules;
      (map[row.id] ||= {})[key] = checked;
    }
  }
  function reset(row: Row) {
    if (!draft.value) return;
    delete (
      row.kind === "tag" ? draft.value.mcpTagRules : draft.value.mcpEntryRules
    )[row.id];
  }
  function toggleTag(id: string) {
    expanded.value.has(id) ? expanded.value.delete(id) : expanded.value.add(id);
  }
  function toggle() {
    open.value = !open.value;
    if (!open.value) query.value = "";
  }
  async function save() {
    if (!draft.value) return;
    const success = await props.v.savePermissions(cloneJson(draft.value));
    if (success && props.v.info) {
      props.v.permissionDraft = null;
      draft.value = cloneJson(props.v.info.settings);
    }
  }
  function revert() {
    if (props.v.info) {
      props.v.permissionDraft = null;
      draft.value = cloneJson(props.v.info.settings);
    }
  }
  const counts = computed(() => ({
    tags: Object.values(draft.value?.mcpTagRules || {}).filter(
      (r) => Object.keys(r).length,
    ).length,
    entries: Object.values(draft.value?.mcpEntryRules || {}).filter(
      (r) => Object.keys(r).length,
    ).length,
  }));
  const allowedCount = computed(
    () =>
      Object.values(projections.value).filter((p) => p.access.allowed).length,
  );
  const selectedPreview = computed(() => projections.value[previewId.value]);
  function preview(id: string) {
    previewId.value = id;
    previewDialog.value?.showModal();
  }

  return {
    projections,
    previewId,
    draft,
    open,
    expanded,
    query,
    previewDialog,
    previewError,
    columns,
    dirty,
    rows,
    own,
    hasRule,
    value,
    source,
    set,
    reset,
    toggleTag,
    toggle,
    save,
    revert,
    counts,
    allowedCount,
    selectedPreview,
    preview,
  };
}
