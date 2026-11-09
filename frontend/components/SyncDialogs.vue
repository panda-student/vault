<script setup lang="ts">
import { ref, watch } from "vue";
import type { UnwrapRef } from "vue";
import type { VaultUI } from "../composables/useVault";
import type { SyncChoice } from "../types/sync";
import { message } from "../utils/errors";
import { syncKinds, syncTime } from "../utils/sync";
const props = defineProps<{ v: UnwrapRef<VaultUI> }>();
const v = props.v;

const requestDialog = ref<HTMLDialogElement | null>(null);
const passphrase = ref("");
const confirm = ref("");
const requestError = ref("");
watch(
  () => v.pendingSync?.id,
  (id) => {
    passphrase.value = "";
    confirm.value = "";
    requestError.value = "";
    if (id) {
      if (!requestDialog.value?.open) requestDialog.value?.showModal();
    } else requestDialog.value?.close();
  },
);
async function answer(cancel: boolean) {
  requestError.value = "";
  if (!cancel) {
    if (!passphrase.value) {
      requestError.value = "请输入同步口令。";
      return;
    }
    if (passphrase.value !== confirm.value) {
      requestError.value = "两次输入的口令不一致。";
      return;
    }
  }
  try {
    await v.respondSync(cancel ? null : passphrase.value);
  } catch (value) {
    // A wrong passphrase keeps the request open: let the user type it again.
    requestError.value = message(value);
    passphrase.value = "";
    confirm.value = "";
  }
}

const conflictDialog = ref<HTMLDialogElement | null>(null);
watch(
  () => v.conflictsWaiting,
  (waiting) => {
    if (waiting) {
      if (!conflictDialog.value?.open) conflictDialog.value?.showModal();
    } else conflictDialog.value?.close();
  },
);
const choices: { value: SyncChoice; label: string; hint: string }[] = [
  {
    value: "localWins",
    label: "本地覆盖云端",
    hint: "以这台电脑为准。",
  },
  {
    value: "remoteWins",
    label: "云端覆盖本地",
    hint: "以云端为准；本机多出的条目移入垃圾桶，可恢复。",
  },
  {
    value: "twoWay",
    label: "双向同步",
    hint: "两边互相补齐；同一条以较新的为准，另一份保留为「冲突副本」。",
  },
];
function postpone() {
  v.postponeConflicts();
  conflictDialog.value?.close();
}
function side(changedAt: number | null, device: string) {
  if (!changedAt) return "无";
  return `${syncTime(changedAt)}${device ? ` · ${device}` : ""}`;
}
</script>

<template>
  <dialog
    ref="requestDialog"
    id="sync-request-dialog"
    @cancel.prevent="answer(true)"
  >
    <form @submit.prevent="answer(false)">
      <h2>AI 请求设置远程同步</h2>
      <p class="dialog-notice">需要你确认同步位置并亲自输入同步口令。</p>
      <dl class="sync-request">
        <dt>方式</dt>
        <dd>{{ v.pendingSync ? syncKinds[v.pendingSync.kind] : "" }}</dd>
        <dt>位置</dt>
        <dd>
          <code>{{ v.pendingSync?.location }}</code>
        </dd>
        <template v-if="v.pendingSync?.credentialName">
          <dt>使用凭据</dt>
          <dd>{{ v.pendingSync.credentialName }}</dd>
        </template>
        <template v-if="v.pendingSync?.create">
          <dt>不存在时</dt>
          <dd>自动创建（GitHub 只建私有仓库）</dd>
        </template>
      </dl>
      <p class="dialog-description">
        确认后程序才会读取该凭据并连接这个位置。远端已有同步文件时，请输入当初设置的口令；口令只保存在本机，不会发送给
        AI。
      </p>
      <div class="form-field">
        <label for="sync-request-passphrase">同步口令</label>
        <input
          id="sync-request-passphrase"
          v-model="passphrase"
          type="password"
          autocomplete="new-password"
          autofocus
        />
      </div>
      <div class="form-field">
        <label for="sync-request-confirm">确认口令</label>
        <input
          id="sync-request-confirm"
          v-model="confirm"
          type="password"
          autocomplete="new-password"
        />
      </div>
      <p v-if="requestError" class="form-error">{{ requestError }}</p>
      <div class="modal-actions">
        <button
          type="button"
          class="secondary"
          :disabled="v.pendingSyncBusy"
          @click="answer(true)"
        >
          拒绝
        </button>
        <button
          id="sync-request-submit"
          class="primary"
          :disabled="v.pendingSyncBusy"
        >
          {{ v.pendingSyncBusy ? "连接中…" : "确认并同步" }}
        </button>
      </div>
    </form>
  </dialog>

  <dialog
    ref="conflictDialog"
    id="sync-conflict-dialog"
    @cancel.prevent="postpone"
  >
    <h2>
      {{ v.syncStatus?.firstJoin ? "本机和云端都已有数据" : "同步发现冲突" }}
    </h2>
    <p class="dialog-description">
      {{
        v.syncStatus?.firstJoin
          ? "这是本机第一次连接这个同步位置，两边的数据不一致。请选择如何合并，选择会作用于整个密码库。"
          : "以下条目在本机和云端都被修改过。其他改动已经同步，这些条目在你选择之前保持不变。"
      }}
    </p>
    <table class="sync-conflicts">
      <thead>
        <tr>
          <th>条目</th>
          <th>本机</th>
          <th>云端</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="c in v.syncStatus?.conflicts ?? []" :key="c.id">
          <td>{{ c.name || "（未命名）" }}</td>
          <td>{{ side(c.localChangedAt, c.localDevice) }}</td>
          <td>{{ side(c.remoteChangedAt, c.remoteDevice) }}</td>
        </tr>
      </tbody>
    </table>
    <div class="sync-choices">
      <button
        v-for="choice in choices"
        :key="choice.value"
        class="secondary"
        :data-choice="choice.value"
        :disabled="v.syncBusy"
        @click="v.resolveSync(choice.value)"
      >
        <strong>{{ choice.label }}</strong>
        <small>{{ choice.hint }}</small>
      </button>
    </div>
    <div class="modal-actions">
      <button class="secondary" :disabled="v.syncBusy" @click="postpone">
        稍后处理
      </button>
    </div>
  </dialog>
</template>

<style scoped>
.sync-request {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 6px 12px;
  margin: 0 0 12px;
  font-size: 12px;
}
.sync-request dt {
  color: #7c8798;
}
.sync-request dd {
  margin: 0;
  overflow-wrap: anywhere;
}
.sync-conflicts {
  width: 100%;
  border-collapse: collapse;
  font-size: 12px;
  margin-bottom: 14px;
  display: block;
  max-height: 30vh;
  overflow: auto;
}
.sync-conflicts th,
.sync-conflicts td {
  text-align: left;
  padding: 6px 8px;
  border-bottom: 1px solid #edf0f5;
}
.sync-conflicts th {
  color: #7c8798;
  font-weight: normal;
}
.sync-choices {
  display: grid;
  gap: 8px;
}
.sync-choices button {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 2px;
  text-align: left;
}
.sync-choices small {
  color: #687387;
  font-size: 11px;
}
</style>
