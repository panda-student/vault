import { onMounted, onUnmounted, ref } from "vue";
import { invoke, pickExportPath, pickImportPath } from "../api/desktop";
import type {
  ExportResult,
  ImportResult,
  PendingExport,
  TransferExportOptions,
  TransferFormat,
  TransferInspection,
} from "../types/transfer";
import type { VaultDependencies } from "./dependencies";
import { message } from "../utils/errors";
export function useTransfer(
  dependencies: Pick<VaultDependencies, "notify" | "refresh">,
) {
  const { notify, refresh } = dependencies;
  const exporting = ref(false);
  const importing = ref(false);
  const pendingExport = ref<PendingExport | null>(null);
  const pendingBusy = ref(false);
  const pendingError = ref("");
  let polling: ReturnType<typeof setInterval> | undefined;
  let pollingNow = false;

  function fileName(format: TransferFormat, encrypted: boolean) {
    const stamp = new Date()
      .toISOString()
      .slice(0, 19)
      .replaceAll(/[-:T]/g, "");
    return `vault-export-${stamp}.${encrypted ? "vaultenc" : format}`;
  }
  async function chooseExportPath(format: TransferFormat, encrypted: boolean) {
    return pickExportPath(fileName(format, encrypted));
  }
  async function chooseImportPath() {
    return pickImportPath();
  }
  async function exportEntries(options: TransferExportOptions) {
    if (exporting.value) return null;
    exporting.value = true;
    try {
      const result = await invoke<ExportResult>("transfer.export", options);
      notify(`已导出 ${result.count} 条凭据`);
      return result;
    } finally {
      exporting.value = false;
    }
  }
  async function inspect(path: string) {
    return invoke<TransferInspection>("transfer.inspect", { path });
  }
  async function importEntries(path: string, passphrase: string | null) {
    if (importing.value) return null;
    importing.value = true;
    try {
      const result = await invoke<ImportResult>("transfer.import", {
        path,
        passphrase,
      });
      await refresh();
      notify(`已导入 ${result.imported} 条凭据`);
      return result;
    } finally {
      importing.value = false;
    }
  }
  async function pollPending() {
    if (pollingNow || pendingBusy.value) return;
    pollingNow = true;
    try {
      const { pending } = await invoke<{ pending: PendingExport | null }>(
        "transfer.pending",
      );
      if (pendingExport.value?.id === pending?.id) return;
      pendingError.value = "";
      pendingExport.value = pending;
    } catch {
      /* The core is momentarily busy or exiting; the next tick retries. */
    } finally {
      pollingNow = false;
    }
  }
  async function respondPending(passphrase: string) {
    const request = pendingExport.value;
    if (!request || pendingBusy.value) return false;
    pendingBusy.value = true;
    pendingError.value = "";
    try {
      const result = await invoke<{ status: string; count?: number }>(
        "transfer.respond",
        {
          id: request.id,
          passphrase,
          cancel: false,
        },
      );
      pendingExport.value = null;
      notify(
        `已加密导出 ${result.count ?? 0} 条凭据到数据目录的 exports 文件夹`,
      );
      return true;
    } catch (value) {
      pendingError.value = message(value);
      notify(pendingError.value);
      return false;
    } finally {
      pendingBusy.value = false;
    }
  }
  async function cancelPending() {
    const request = pendingExport.value;
    if (!request || pendingBusy.value) return false;
    pendingBusy.value = true;
    pendingError.value = "";
    try {
      await invoke("transfer.respond", {
        id: request.id,
        passphrase: null,
        cancel: true,
      });
      pendingExport.value = null;
      notify("已取消导出");
      return true;
    } catch (value) {
      pendingError.value = message(value);
      notify(pendingError.value);
      return false;
    } finally {
      pendingBusy.value = false;
    }
  }
  onMounted(() => {
    void pollPending();
    polling = setInterval(() => void pollPending(), 2000);
  });
  onUnmounted(() => clearInterval(polling));
  return {
    exporting,
    importing,
    pendingExport,
    pendingBusy,
    pendingError,
    chooseExportPath,
    chooseImportPath,
    exportEntries,
    inspect,
    importEntries,
    respondPending,
    cancelPending,
  };
}
