import { computed, ref } from "vue";
import type {
  DatabaseFields,
  Kind,
  Entry,
  Organization,
  TrashEntry,
  Settings,
  Info,
  ApiService,
} from "../types/models";
import { protocolName } from "../utils/display";
export function createVaultState() {
  const page = ref<"entries" | "trash" | "settings" | "mcp" | "updates">(
    "entries",
  );
  const tagFilter = ref(""),
    typeFilter = ref<Kind | "">("");
  const organization = ref<Organization>({ tags: [] }),
    trash = ref<TrashEntry[]>([]);
  const query = ref(""),
    entries = ref<Entry[]>([]),
    selected = ref<string | null>(null),
    revealed = ref<string | null>(null);
  const revealEpoch = ref(0);
  const info = ref<Info | null>(null),
    loading = ref(true),
    error = ref(""),
    toast = ref(""),
    saving = ref(false),
    configVisible = ref(false);
  const settingsSaving = ref(false);
  const permissionDraft = ref<Settings | null>(null);
  const deleteDialog = ref<HTMLDialogElement | null>(null);
  const editing = ref<Entry | null>(null),
    pendingDelete = ref<Entry | null>(null),
    formKind = ref<Kind>("password"),
    formError = ref("");
  const form = ref({
    title: "",
    account: "",
    url: "",
    documentationUrl: "",
    secret: "",
    notes: "",
    tags: [] as string[],
    database: {
      databaseType: "MySQL",
      host: "",
      port: "3306",
      databaseName: "",
    } as DatabaseFields,
    services: [] as ApiService[],
  });
  const creating = ref(false),
    baseline = ref("");
  const dirty = computed(() => JSON.stringify(form.value) !== baseline.value);
  type Draft = {
    entry: Entry | null;
    kind: Kind;
    fields: typeof form.value;
    baseline: string;
  };
  const drafts = new Map<string, Draft>();
  const switching = ref(false);
  const list = computed(() =>
    entries.value.filter(
      (e) =>
        (!tagFilter.value || e.tags.includes(tagFilter.value)) &&
        (!typeFilter.value || e.kind === typeFilter.value) &&
        [
          e.title,
          e.account,
          e.url,
          e.documentationUrl,
          ...e.tags,
          ...Object.values(e.database ?? {}),
          ...e.services.flatMap((s) => [s.url, protocolName(s.protocol)]),
        ].some((v) =>
          v.toLowerCase().includes(query.value.trim().toLowerCase()),
        ),
    ),
  );
  const detail = computed(() =>
    entries.value.find((e) => e.id === selected.value),
  );
  const title = computed(() =>
    page.value === "updates"
      ? "软件更新"
      : page.value === "settings"
        ? "设置"
        : page.value === "mcp"
          ? "MCP 管理"
          : page.value === "trash"
            ? "垃圾桶"
            : tagFilter.value
              ? "# " + tagFilter.value
              : "全部凭据",
  );
  const config = computed(() =>
    info.value
      ? JSON.stringify(
          {
            mcpServers: {
              vault: {
                command: info.value.executable,
                args: ["--mcp", "--data-dir", info.value.dataDirectory],
              },
            },
          },
          null,
          2,
        )
      : "",
  );

  return {
    page,
    tagFilter,
    typeFilter,
    organization,
    trash,
    query,
    entries,
    selected,
    revealed,
    revealEpoch,
    info,
    loading,
    error,
    toast,
    saving,
    configVisible,
    settingsSaving,
    permissionDraft,
    deleteDialog,
    editing,
    pendingDelete,
    formKind,
    formError,
    form,
    creating,
    baseline,
    dirty,
    drafts,
    switching,
    list,
    detail,
    title,
    config,
  };
}
export type VaultState = ReturnType<typeof createVaultState>;
