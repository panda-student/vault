<script setup lang="ts">
import { reactive, ref, watch } from "vue";
import { useVault } from "./composables/useVault";
import VaultSidebar from "./components/VaultSidebar.vue";
import CredentialsPage from "./pages/CredentialsPage.vue";
import TrashPage from "./pages/TrashPage.vue";
import McpPage from "./pages/McpPage.vue";
import SettingsPage from "./pages/SettingsPage.vue";
import UpdatesPage from "./pages/UpdatesPage.vue";
const state = useVault();
const v = reactive(state);
const { deleteDialog } = state;
const organizationDialog = ref<HTMLDialogElement | null>(null),
  organizationKind = ref<"tag">("tag");
const organizationName = ref(""),
  previousName = ref<string | undefined>(),
  organizationError = ref(""),
  organizing = ref(false);
const deleteTagDialog = ref<HTMLDialogElement | null>(null);
const mcpExportDialog = ref<HTMLDialogElement | null>(null);
const mcpPassphrase = ref("");
const mcpConfirm = ref("");
const mcpError = ref("");
const mcpBusy = ref(false);
watch(
  () => v.pendingExport?.id,
  (id) => {
    if (id) {
      mcpPassphrase.value = "";
      mcpConfirm.value = "";
      mcpError.value = "";
      if (!mcpExportDialog.value?.open) mcpExportDialog.value?.showModal();
    } else mcpExportDialog.value?.close();
  },
);
async function submitMcpExport() {
  if (!v.pendingExport || mcpBusy.value) return;
  mcpError.value = "";
  if (!mcpPassphrase.value) {
    mcpError.value = "请输入导出口令。";
    return;
  }
  if (mcpPassphrase.value !== mcpConfirm.value) {
    mcpError.value = "两次输入的口令不一致。";
    return;
  }
  mcpBusy.value = true;
  try {
    if (!(await v.respondPending(mcpPassphrase.value)))
      mcpError.value = v.pendingError || "导出未完成，请重试。";
  } finally {
    mcpBusy.value = false;
  }
}
async function cancelMcpExport() {
  if (!v.pendingExport || mcpBusy.value) return;
  mcpBusy.value = true;
  try {
    if (!(await v.cancelPending()))
      mcpError.value = v.pendingError || "取消未完成，请重试。";
  } finally {
    mcpBusy.value = false;
  }
}
async function deleteTag() {
  if (!previousName.value) return;
  organizing.value = true;
  try {
    await v.organize("tag", "", previousName.value);
    deleteTagDialog.value?.close();
    organizationDialog.value?.close();
    v.notify("已删除标签及对应权限规则，凭据继续保留。");
  } catch (e) {
    organizationError.value = (e as { message?: string }).message || "删除失败";
    deleteTagDialog.value?.close();
  } finally {
    organizing.value = false;
  }
}
function openOrganization(kind: "tag", previous?: string) {
  organizationKind.value = kind;
  previousName.value = previous;
  organizationName.value = previous ?? "";
  organizationError.value = "";
  organizationDialog.value?.showModal();
}
async function saveOrganization() {
  if (organizing.value) return;
  organizing.value = true;
  try {
    await v.organize(
      organizationKind.value,
      organizationName.value,
      previousName.value,
    );
    organizationDialog.value?.close();
  } catch (e) {
    organizationError.value = (e as { message?: string }).message || "保存失败";
  } finally {
    organizing.value = false;
  }
}
</script>
<template>
  <div class="app-shell">
    <VaultSidebar :v="v" @organize="openOrganization" />
    <main>
      <header>
        <div>
          <h1>{{ v.title }}</h1>
          <p>本地加密保存，AI 按引用填写。</p>
        </div>
        <button
          v-if="v.page === 'entries'"
          id="add-btn"
          class="primary"
          :disabled="v.loading || !!v.error || v.saving"
          @click="v.openForm()"
        >
          ＋ 新增凭据
        </button>
      </header>
      <div v-if="v.error" class="error" role="alert">
        {{ v.error }}<button class="text-btn" @click="v.refresh()">重试</button>
      </div>
      <CredentialsPage
        v-if="v.page === 'entries'"
        :v="v"
        @organize="openOrganization"
      />
      <TrashPage v-else-if="v.page === 'trash'" :v="v" />
      <McpPage v-else-if="v.page === 'mcp'" :v="v" />
      <UpdatesPage v-else-if="v.page === 'updates'" :v="v" />
      <SettingsPage v-else :v="v" />
      <footer>
        <span
          ><span class="dot"></span
          >{{ v.info ? "本地密码库已解锁" : "正在连接本地核心" }}</span
        ><button
          v-if="['available', 'ready'].includes(v.updater.status.phase)"
          class="text-btn"
          id="update-notice"
          @click="v.navigate('updates')"
        >
          {{ v.updater.status.phase === "ready" ? "更新已就绪" : "发现新版本" }}
          · v{{ v.updater.status.version }}</button
        ><span>v{{ v.info?.version }}</span>
      </footer>
    </main>
  </div>
  <dialog ref="deleteDialog" id="delete-dialog">
    <h2>移入垃圾桶？</h2>
    <p class="dialog-description">
      “{{
        v.pendingDelete?.title
      }}”将从正常列表移除。你可以在垃圾桶恢复。未保存的修改不会保留。
    </p>
    <div class="modal-actions">
      <button
        id="cancel-delete"
        autofocus
        class="secondary"
        :disabled="v.saving"
        @click="deleteDialog?.close()"
      >
        取消</button
      ><button
        id="confirm-delete"
        class="primary danger-fill"
        :disabled="v.saving"
        @click="v.confirmDelete()"
      >
        移入垃圾桶
      </button>
    </div>
  </dialog>
  <dialog ref="organizationDialog" id="organization-dialog">
    <form @submit.prevent="saveOrganization">
      <h2>{{ previousName ? "重命名" : "新增" }}标签</h2>
      <div class="form-field" style="margin-top: 20px">
        <label for="organization-name">名称</label
        ><input
          id="organization-name"
          v-model="organizationName"
          required
          maxlength="64"
          autofocus
        />
      </div>
      <p v-if="organizationError" class="form-error">{{ organizationError }}</p>
      <div class="modal-actions">
        <button
          v-if="previousName"
          id="delete-tag"
          type="button"
          class="danger"
          :disabled="organizing"
          @click="deleteTagDialog?.showModal()"
        >
          删除标签</button
        ><button
          type="button"
          class="secondary"
          :disabled="organizing"
          @click="organizationDialog?.close()"
        >
          取消</button
        ><button class="primary" :disabled="organizing">保存</button>
      </div>
    </form>
  </dialog>
  <dialog ref="deleteTagDialog" id="delete-tag-dialog">
    <h2>删除标签？</h2>
    <p class="dialog-description">
      移除“{{
        previousName
      }}”及对应权限规则。凭据不会删除；受影响操作会按其他标签或默认规则重新计算。
    </p>
    <div class="modal-actions">
      <button
        class="secondary"
        :disabled="organizing"
        @click="deleteTagDialog?.close()"
      >
        取消</button
      ><button
        class="primary danger-fill"
        :disabled="organizing"
        @click="deleteTag"
      >
        删除标签
      </button>
    </div>
  </dialog>
  <dialog
    ref="mcpExportDialog"
    id="mcp-export-dialog"
    @cancel.prevent="cancelMcpExport"
  >
    <form @submit.prevent="submitMcpExport">
      <h2>AI 请求加密导出</h2>
      <p class="dialog-description">
        AI 请求导出 {{ v.pendingExport?.count ?? 0 }} 条{{
          v.pendingExport?.includeTrash ? "（含垃圾桶）" : ""
        }}凭据。请设置导出口令；口令只在本地参与加密，不会发送给
        AI。文件将保存到数据目录的 exports 文件夹。
      </p>
      <div class="form-field">
        <label for="mcp-export-passphrase">口令</label
        ><input
          id="mcp-export-passphrase"
          v-model="mcpPassphrase"
          type="password"
          autocomplete="new-password"
          autofocus
        />
      </div>
      <div class="form-field">
        <label for="mcp-export-confirm">确认口令</label
        ><input
          id="mcp-export-confirm"
          v-model="mcpConfirm"
          type="password"
          autocomplete="new-password"
        />
      </div>
      <p class="dialog-description" style="margin: -6px 0 0">
        口令不会保存。忘记口令将无法恢复导出文件。
      </p>
      <p v-if="mcpError" class="form-error">{{ mcpError }}</p>
      <div class="modal-actions">
        <button
          type="button"
          class="secondary"
          :disabled="mcpBusy"
          @click="cancelMcpExport"
        >
          取消导出</button
        ><button id="mcp-export-submit" class="primary" :disabled="mcpBusy">
          {{ mcpBusy ? "处理中…" : "确认导出" }}
        </button>
      </div>
    </form>
  </dialog>
  <div v-if="v.toast" class="toast" role="status">{{ v.toast }}</div>
</template>
