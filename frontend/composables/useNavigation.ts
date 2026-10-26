import { onMounted, onUnmounted } from "vue";
import type { VaultState } from "./state";
import type { VaultDependencies } from "./dependencies";
export function useNavigation(
  state: Pick<
    VaultState,
    | "page"
    | "query"
    | "toast"
    | "saving"
    | "permissionDraft"
    | "creating"
    | "detail"
  >,
  dependencies: Pick<VaultDependencies, "hide" | "stashDraft" | "loadEntry">,
) {
  const { page, query, toast, saving, permissionDraft, creating, detail } =
    state;
  const { hide, stashDraft, loadEntry } = dependencies;
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  onUnmounted(() => clearTimeout(toastTimer));
  onMounted(() => window.addEventListener("keydown", keyboard));
  onUnmounted(() => window.removeEventListener("keydown", keyboard));
  function notify(value: string) {
    toast.value = value;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => {
      toast.value = "";
    }, 3000);
  }
  function navigate(value: typeof page.value) {
    if (saving.value) return;
    if (page.value === "mcp" && permissionDraft.value && value !== "mcp")
      notify("未应用的权限修改已保留，返回 MCP 管理可继续编辑。");
    stashDraft();
    const same = value === page.value;
    creating.value = false;
    page.value = value;
    query.value = "";
    hide();
    if (same) loadEntry(detail.value);
  }
  function keyboard(e: KeyboardEvent) {
    if (e.ctrlKey && e.key === "k") {
      e.preventDefault();
      document.querySelector<HTMLInputElement>("#search")?.focus();
    }
  }
  return { notify, navigate };
}
