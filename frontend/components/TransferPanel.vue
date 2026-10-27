<script setup lang="ts">
import { reactive, ref } from "vue";
import type { UnwrapRef } from "vue";
import type { VaultUI } from "../composables/useVault";
import type { Kind } from "../types/models";
import type { TransferFormat, TransferInspection } from "../types/transfer";
import { message } from "../utils/errors";
const props = defineProps<{ v: UnwrapRef<VaultUI> }>();
const v = props.v;
const kinds: { value: Kind; label: string }[] = [
  { value: "password", label: "网站密码" },
  { value: "api", label: "API 密钥" },
  { value: "database", label: "数据库" },
  { value: "generic", label: "其他" },
];
const allKinds = kinds.map((kind) => kind.value);
const exportDialog = ref<HTMLDialogElement | null>(null);
const plaintextDialog = ref<HTMLDialogElement | null>(null);
const importDialog = ref<HTMLDialogElement | null>(null);
const draft = reactive({
  kinds: [...allKinds] as Kind[],
  includeTrash: false,
  format: "json" as TransferFormat,
  encrypted: true,
  passphrase: "",
  confirm: "",
});
const exportError = ref("");
const targetPath = ref("");
const inspection = ref<TransferInspection | null>(null);
const importPath = ref("");
const importPassphrase = ref("");
const importError = ref("");
const importLoading = ref(false);
function resetExport() {
  draft.kinds = [...allKinds];
  draft.includeTrash = false;
  draft.format = "json";
  draft.encrypted = true;
  draft.passphrase = "";
  draft.confirm = "";
  exportError.value = "";
  targetPath.value = "";
}
function openExport() {
  resetExport();
  exportDialog.value?.showModal();
}
async function startExport() {
  exportError.value = "";
  if (!draft.kinds.length) {
    exportError.value = "请至少选择一种凭据类型。";
    return;
  }
  if (draft.encrypted) {
    if (!draft.passphrase) {
      exportError.value = "请输入导出口令。";
      return;
    }
    if (draft.passphrase !== draft.confirm) {
      exportError.value = "两次输入的口令不一致。";
      return;
    }
  }
  const path = await v.chooseExportPath(draft.format, draft.encrypted);
  if (!path) return;
  targetPath.value = path;
  if (draft.encrypted) await runExport();
  else plaintextDialog.value?.showModal();
}
async function runExport() {
  try {
    const result = await v.exportEntries({
      format: draft.format,
      includeTrash: draft.includeTrash,
      kinds: [...draft.kinds],
      encrypted: draft.encrypted,
      passphrase: draft.encrypted ? draft.passphrase : null,
      path: targetPath.value,
    });
    if (!result) return;
    plaintextDialog.value?.close();
    exportDialog.value?.close();
    resetExport();
  } catch (value) {
    plaintextDialog.value?.close();
    exportError.value = message(value);
  }
}
function resetImport() {
  inspection.value = null;
  importPath.value = "";
  importPassphrase.value = "";
  importError.value = "";
  importLoading.value = false;
}
async function openImport() {
  resetImport();
  const path = await v.chooseImportPath();
  if (!path) return;
  importPath.value = path;
  importLoading.value = true;
  importDialog.value?.showModal();
  try {
    inspection.value = await v.inspect(path);
  } catch (value) {
    importError.value = message(value);
  } finally {
    importLoading.value = false;
  }
}
async function submitImport() {
  if (v.importing || importLoading.value) return;
  importError.value = "";
  if (!inspection.value) return;
  if (inspection.value.encrypted && !importPassphrase.value) {
    importError.value = "请输入导出口令。";
    return;
  }
  try {
    const result = await v.importEntries(
      importPath.value,
      inspection.value.encrypted ? importPassphrase.value : null,
    );
    if (!result) return;
    importDialog.value?.close();
    resetImport();
  } catch (value) {
    importError.value = message(value);
  }
}
</script>

<template>
  <div class="card transfer-card">
    <h3>导入与导出</h3>
    <div class="setting-row">
      <div>
        导出凭据
        <small
          >按类型选择，可选包含垃圾桶；加密导出使用 AES-256-GCM 与
          argon2id，需要设置口令。</small
        >
      </div>
      <button
        id="export-open"
        class="secondary"
        :disabled="v.exporting || v.importing"
        @click="openExport"
      >
        导出…
      </button>
    </div>
    <div class="setting-row">
      <div>
        导入凭据
        <small
          >支持本应用 JSON/CSV、Chrome 与 Edge
          密码文件；全部新建，不覆盖或合并现有凭据；任一行无效时整批回滚。</small
        >
      </div>
      <button
        id="import-open"
        class="secondary"
        :disabled="v.exporting || v.importing"
        @click="openImport"
      >
        导入…
      </button>
    </div>
  </div>

  <dialog ref="exportDialog" id="export-dialog">
    <form @submit.prevent="startExport">
      <h2>导出凭据</h2>
      <p class="dialog-description">
        选择要导出的内容。导出文件不会自动删除，请妥善保管。
      </p>
      <fieldset class="transfer-kinds">
        <legend>凭据类型</legend>
        <label v-for="kind in kinds" :key="kind.value">
          <input
            :id="`export-kind-${kind.value}`"
            v-model="draft.kinds"
            type="checkbox"
            :value="kind.value"
          />
          {{ kind.label }}
        </label>
      </fieldset>
      <label class="transfer-check">
        <input id="export-trash" v-model="draft.includeTrash" type="checkbox" />
        包含垃圾桶中的凭据
      </label>
      <div class="form-field">
        <label for="export-format">文件格式</label>
        <select id="export-format" v-model="draft.format">
          <option value="json">JSON（保留全部字段）</option>
          <option value="csv">CSV（兼容 Chrome / Edge）</option>
        </select>
      </div>
      <label class="transfer-check">
        <input
          id="export-encrypted"
          v-model="draft.encrypted"
          type="checkbox"
        />
        使用口令加密（推荐）
      </label>
      <template v-if="draft.encrypted">
        <div class="form-field">
          <label for="export-passphrase">口令</label>
          <input
            id="export-passphrase"
            v-model="draft.passphrase"
            type="password"
            autocomplete="new-password"
          />
        </div>
        <div class="form-field">
          <label for="export-confirm">确认口令</label>
          <input
            id="export-confirm"
            v-model="draft.confirm"
            type="password"
            autocomplete="new-password"
          />
        </div>
        <p class="transfer-hint">口令不会保存。忘记口令将无法恢复导出文件。</p>
      </template>
      <p v-else class="transfer-warning">
        未加密文件中的凭据以明文保存，任何能读取该文件的人都可以看到。
      </p>
      <p v-if="exportError" class="form-error">{{ exportError }}</p>
      <div class="modal-actions">
        <button
          type="button"
          class="secondary"
          :disabled="v.exporting"
          autofocus
          @click="exportDialog?.close()"
        >
          取消
        </button>
        <button id="export-submit" class="primary" :disabled="v.exporting">
          {{ v.exporting ? "导出中…" : "选择位置并导出" }}
        </button>
      </div>
    </form>
  </dialog>

  <dialog ref="plaintextDialog" id="plaintext-dialog">
    <h2>导出未加密文件？</h2>
    <p class="dialog-description">
      文件中的口令与密钥将以明文保存。请仅保存到可信位置，用完后及时删除。
    </p>
    <p v-if="exportError" class="form-error">{{ exportError }}</p>
    <div class="modal-actions">
      <button
        class="secondary"
        :disabled="v.exporting"
        autofocus
        @click="plaintextDialog?.close()"
      >
        取消
      </button>
      <button
        id="plaintext-confirm"
        class="primary danger-fill"
        :disabled="v.exporting"
        @click="runExport"
      >
        继续导出
      </button>
    </div>
  </dialog>

  <dialog ref="importDialog" id="import-dialog">
    <h2>导入凭据</h2>
    <p v-if="importLoading" class="dialog-description">正在检查文件…</p>
    <template v-else>
      <p v-if="inspection" class="dialog-description">
        {{
          inspection.encrypted
            ? "文件已加密，请输入导出时设置的口令。"
            : `文件包含 ${inspection.count ?? 0} 条凭据，将以新条目全部导入，不覆盖现有数据。`
        }}
      </p>
      <div v-if="inspection?.encrypted" class="form-field">
        <label for="import-passphrase">口令</label>
        <input
          id="import-passphrase"
          v-model="importPassphrase"
          type="password"
          autocomplete="current-password"
        />
      </div>
      <p v-if="importError" class="form-error">{{ importError }}</p>
      <div class="modal-actions">
        <button
          class="secondary"
          :disabled="v.importing"
          autofocus
          @click="importDialog?.close()"
        >
          取消
        </button>
        <button
          id="import-submit"
          class="primary"
          :disabled="
            v.importing ||
            !inspection ||
            (!inspection.encrypted && !inspection.count)
          "
          @click="submitImport"
        >
          {{ v.importing ? "导入中…" : "导入" }}
        </button>
      </div>
    </template>
  </dialog>
</template>

<style scoped>
.transfer-kinds {
  border: 0;
  padding: 0;
  margin: 0 0 14px;
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 9px 12px;
}
.transfer-kinds legend {
  padding: 0;
  margin-bottom: 8px;
  color: #7c8798;
  font-size: 11px;
}
.transfer-kinds label,
.transfer-check {
  display: flex;
  align-items: center;
  gap: 8px;
  color: #59657a;
  font-size: 12px;
  line-height: 1.6;
}
.transfer-kinds input,
.transfer-check input {
  width: auto;
  margin: 0;
  accent-color: var(--accent);
}
.transfer-check {
  margin: 0 0 16px;
}
.transfer-hint {
  margin: -4px 0 14px;
  color: #687387;
  font-size: 11px;
  line-height: 1.8;
}
.transfer-warning {
  margin: 0 0 14px;
  padding: 9px 11px;
  border-radius: 6px;
  background: #fff1f1;
  color: #9d4657;
  font-size: 11px;
  line-height: 1.7;
}
#export-format {
  width: 100%;
  border: 1px solid #dde2eb;
  border-radius: 6px;
  padding: 10px;
  background: #fcfdff;
  color: #344057;
  font: inherit;
}
</style>
