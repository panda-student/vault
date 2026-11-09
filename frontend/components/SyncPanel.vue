<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import type { UnwrapRef } from "vue";
import type { VaultUI } from "../composables/useVault";
import type {
  CredentialRef,
  SyncKind,
  SyncSetup,
  SyncTarget,
} from "../types/sync";
import { message } from "../utils/errors";
import {
  DEFAULT_SYNC_PATH,
  githubAddress,
  parseGitHubRepo,
  sameSyncTarget,
  syncKinds,
  syncSummary,
} from "../utils/sync";
const props = defineProps<{ v: UnwrapRef<VaultUI> }>();
const v = props.v;
const setupDialog = ref<HTMLDialogElement | null>(null);
const disableDialog = ref<HTMLDialogElement | null>(null);
const draft = reactive({
  kind: "folder" as SyncKind,
  directory: "",
  url: "",
  repoUrl: "",
  path: DEFAULT_SYNC_PATH,
  credential: "",
  create: false,
  passphrase: "",
  confirm: "",
  intervalMinutes: 15 as number | "",
  deviceName: "",
});
/** The saved reference, sent back unchanged when the user keeps it. */
const savedRef = ref<CredentialRef | null>(null);
const setupError = ref("");
const testing = ref(false);
/** Known from a connection test; an existing file needs its original passphrase. */
const remoteExists = ref<boolean | null>(null);
const status = computed(() => v.syncStatus);

/** Entries whose groups can supply what the chosen backend needs. */
const credentialOptions = computed(() =>
  v.syncCredentials.flatMap((entry) =>
    entry.groups
      .filter((g) =>
        draft.kind === "webdav" ? g.hasPassword : g.hasToken || g.hasPassword,
      )
      .map((g) => ({
        value: `${entry.id}::${g.id}`,
        label: [entry.name, g.environment, g.account]
          .filter(Boolean)
          .join(" · "),
      })),
  ),
);

/** The group an entry uses when no group is named: its only one. */
function onlyGroup(id: string) {
  const groups = v.syncCredentials.find((e) => e.id === id)?.groups ?? [];
  return groups.length === 1 ? groups[0].id : undefined;
}
/** Name a group only when the entry has several, the way the saved one does;
 *  another spelling of the same choice would count as a new target. */
function reference(): CredentialRef {
  const [credentialId, groupId] = draft.credential.split("::");
  const saved = savedRef.value;
  if (
    saved?.credentialId === credentialId &&
    (saved.groupId ?? onlyGroup(credentialId)) === groupId
  )
    return { ...saved };
  return groupId && groupId !== onlyGroup(credentialId)
    ? { credentialId, groupId }
    : { credentialId };
}
function target(): SyncTarget {
  const path = draft.path.trim() || DEFAULT_SYNC_PATH;
  if (draft.kind === "folder")
    return { kind: "folder", directory: draft.directory.trim(), path };
  if (draft.kind === "webdav")
    return {
      kind: "webdav",
      url: draft.url.trim(),
      path,
      credential: reference(),
    };
  const repo = parseGitHubRepo(draft.repoUrl);
  return {
    kind: "github",
    owner: repo?.owner ?? "",
    repo: repo?.repo ?? "",
    branch: repo?.branch,
    path,
    token: reference(),
  };
}
/** What the repository address reads as, shown under the field. */
const parsedRepo = computed(() => parseGitHubRepo(draft.repoUrl));
/** Same remote as the saved one: the saved passphrase may be kept. */
const sameTarget = computed(
  () => !!status.value?.target && sameSyncTarget(status.value.target, target()),
);
// A connection test only describes the target it was run against.
watch(
  () => JSON.stringify(target()),
  () => (remoteExists.value = null),
);
function setup(): SyncSetup {
  return {
    target: target(),
    passphrase: draft.passphrase || undefined,
    intervalMinutes:
      draft.intervalMinutes === "" ? undefined : draft.intervalMinutes,
    deviceName: draft.deviceName.trim() || undefined,
    create: draft.create,
  };
}

async function openSetup() {
  const current = status.value;
  const saved = current?.target;
  Object.assign(draft, {
    kind: saved?.kind ?? "folder",
    directory: saved?.kind === "folder" ? saved.directory : "",
    url: saved?.kind === "webdav" ? saved.url : "",
    repoUrl: saved?.kind === "github" ? githubAddress(saved) : "",
    path: saved?.path ?? DEFAULT_SYNC_PATH,
    credential: "",
    create: false,
    passphrase: "",
    confirm: "",
    intervalMinutes: current?.intervalMinutes ?? 15,
    deviceName: current?.deviceName ?? "",
  });
  const credential =
    saved?.kind === "webdav"
      ? saved.credential
      : saved?.kind === "github"
        ? saved.token
        : null;
  savedRef.value = credential;
  setupError.value = "";
  setupDialog.value?.showModal();
  try {
    await v.loadSyncCredentials();
    // Select the saved group; without a group id it is the entry's only one.
    if (credential)
      draft.credential = `${credential.credentialId}::${
        credential.groupId ?? onlyGroup(credential.credentialId) ?? ""
      }`;
    remoteExists.value = null;
  } catch (value) {
    setupError.value = message(value);
  }
}
async function chooseFolder() {
  const folder = await v.pickSyncFolder();
  if (folder) draft.directory = folder;
}
function invalid(): string {
  if (draft.kind === "folder" && !draft.directory.trim())
    return "请选择同步文件夹。";
  if (draft.kind === "webdav" && !draft.url.trim())
    return "请填写 WebDAV 地址。";
  if (draft.kind === "github" && !parsedRepo.value)
    return "请填写仓库地址，例如 https://github.com/所有者/仓库名。";
  if (draft.kind !== "folder" && !draft.credential)
    return "请选择库里保存账号或 Token 的凭据。";
  return "";
}
async function test() {
  setupError.value = invalid();
  if (setupError.value) return;
  testing.value = true;
  try {
    remoteExists.value = (await v.testSync(setup())).remoteExists;
  } catch (value) {
    remoteExists.value = null;
    setupError.value = message(value);
  } finally {
    testing.value = false;
  }
}
async function save() {
  setupError.value = invalid();
  if (setupError.value) return;
  if (!sameTarget.value || draft.passphrase) {
    if (!draft.passphrase) {
      setupError.value = "请输入同步口令。";
      return;
    }
    if (remoteExists.value !== true && draft.passphrase !== draft.confirm) {
      setupError.value = "两次输入的口令不一致。";
      return;
    }
  }
  try {
    const result = await v.configureSync(setup());
    if (!result) return;
    setupDialog.value?.close();
    v.notify(
      result.error
        ? `已保存，但同步失败：${result.error.message}`
        : "同步已设置",
    );
  } catch (value) {
    setupError.value = message(value);
  }
}
async function disable() {
  if (await v.disableSync()) disableDialog.value?.close();
}
</script>

<template>
  <div class="card sync-card">
    <h3>远程同步</h3>
    <template v-if="!status?.configured">
      <div class="setting-row">
        <div>
          在多台电脑之间同步密码库
          <small
            >加密后存到网盘文件夹、WebDAV 或 GitHub
            私有仓库；只读写一个同步文件，不碰远端其他内容。</small
          >
        </div>
        <button
          id="sync-setup"
          class="secondary"
          :disabled="!status"
          @click="openSetup"
        >
          设置同步…
        </button>
      </div>
    </template>
    <template v-else>
      <div class="setting-row vertical">
        <span>同步位置</span>
        <code id="sync-location"
          >{{ syncKinds[status.target!.kind] }} · {{ status.location }}</code
        >
        <small v-if="status.credentialName"
          >使用库中的凭据「{{ status.credentialName }}」</small
        >
      </div>
      <div class="setting-row">
        <span>本机名称 · 间隔</span>
        <span
          >{{ status.deviceName }} ·
          {{
            status.intervalMinutes
              ? `每 ${status.intervalMinutes} 分钟`
              : "仅修改后和手动"
          }}</span
        >
      </div>
      <div class="setting-row">
        <span>状态</span>
        <span id="sync-state" :class="{ 'sync-error': status.error }">{{
          syncSummary(status)
        }}</span>
      </div>
      <div v-if="status.conflicts.length" class="setting-row">
        <span class="sync-error"
          >{{ status.conflicts.length }} 条{{
            status.firstJoin ? "首次同步差异" : "冲突"
          }}等待你的选择</span
        >
        <button class="text-btn" @click="v.reviewConflicts()">处理冲突</button>
      </div>
      <div class="sync-actions">
        <button
          id="sync-run"
          class="primary"
          :disabled="v.syncBusy || status.running"
          @click="v.runSync()"
        >
          {{ v.syncBusy || status.running ? "同步中…" : "立即同步" }}
        </button>
        <button class="secondary" :disabled="v.syncBusy" @click="openSetup">
          修改…
        </button>
        <button
          class="danger"
          :disabled="v.syncBusy"
          @click="disableDialog?.showModal()"
        >
          停用
        </button>
      </div>
    </template>
  </div>

  <dialog ref="setupDialog" id="sync-dialog">
    <form @submit.prevent="save">
      <h2>设置远程同步</h2>
      <p class="dialog-description">
        同步文件用你设置的口令端到端加密。远端已有同步文件时，必须输入原来的口令，验证通过后才会写入本机。
      </p>
      <div class="form-field">
        <label for="sync-kind">同步方式</label>
        <select id="sync-kind" v-model="draft.kind">
          <option v-for="(label, kind) in syncKinds" :key="kind" :value="kind">
            {{ label }}
          </option>
        </select>
      </div>
      <div v-if="draft.kind === 'folder'" class="form-field">
        <label for="sync-directory">文件夹</label>
        <div class="sync-inline">
          <input
            id="sync-directory"
            v-model="draft.directory"
            placeholder="例如网盘客户端同步的文件夹"
          />
          <button type="button" class="secondary" @click="chooseFolder">
            选择…
          </button>
        </div>
      </div>
      <template v-else>
        <template v-if="draft.kind === 'webdav'">
          <div class="form-field">
            <label for="sync-url">WebDAV 地址</label>
            <input
              id="sync-url"
              v-model="draft.url"
              placeholder="https://dav.example.com/remote.php/webdav"
            />
          </div>
        </template>
        <template v-else>
          <div class="form-field">
            <label for="sync-repo-url">仓库地址</label>
            <input
              id="sync-repo-url"
              v-model="draft.repoUrl"
              placeholder="https://github.com/所有者/仓库名"
            />
            <small class="sync-hint">
              {{
                parsedRepo
                  ? `将使用 ${parsedRepo.owner}/${parsedRepo.repo}${parsedRepo.branch ? `，分支 ${parsedRepo.branch}` : "，默认分支"}`
                  : "粘贴仓库链接即可，可以带 /tree/分支名 指定分支"
              }}
            </small>
          </div>
        </template>
        <div class="form-field">
          <label for="sync-credential">{{
            draft.kind === "webdav" ? "账号与密码" : "访问 Token"
          }}</label>
          <select id="sync-credential" v-model="draft.credential">
            <option value="" disabled>从库中选择凭据</option>
            <option
              v-for="option in credentialOptions"
              :key="option.value"
              :value="option.value"
            >
              {{ option.label }}
            </option>
          </select>
          <small class="sync-hint"
            >密码和 Token
            只在本机使用，不保存在同步设置里；在库中轮换后自动生效。</small
          >
        </div>
      </template>
      <label v-if="draft.kind !== 'webdav'" class="sync-check">
        <input v-model="draft.create" type="checkbox" />
        {{
          draft.kind === "github"
            ? "仓库不存在时新建私有仓库"
            : "文件夹不存在时创建"
        }}
      </label>
      <div class="form-field">
        <label for="sync-path">同步文件路径</label>
        <input id="sync-path" v-model="draft.path" />
      </div>
      <div class="form-field">
        <label for="sync-passphrase">同步口令</label>
        <input
          id="sync-passphrase"
          v-model="draft.passphrase"
          type="password"
          autocomplete="new-password"
          :placeholder="sameTarget ? '留空保留已保存的口令' : ''"
        />
      </div>
      <div
        v-if="remoteExists !== true && (!sameTarget || draft.passphrase)"
        class="form-field"
      >
        <label for="sync-confirm">确认口令</label>
        <input
          id="sync-confirm"
          v-model="draft.confirm"
          type="password"
          autocomplete="new-password"
        />
      </div>
      <p v-if="remoteExists !== null" class="sync-hint">
        {{
          remoteExists
            ? "远端已有同步文件：请输入当初设置的口令。"
            : "远端还没有同步文件：将用这个口令新建。"
        }}
      </p>
      <div class="sync-grid">
        <div class="form-field">
          <label for="sync-interval">同步间隔（分钟，0 为仅修改后）</label>
          <input
            id="sync-interval"
            v-model.number="draft.intervalMinutes"
            type="number"
            min="0"
            max="1440"
          />
        </div>
        <div class="form-field">
          <label for="sync-device">本机名称</label>
          <input
            id="sync-device"
            v-model="draft.deviceName"
            maxlength="64"
            placeholder="默认使用电脑名"
          />
        </div>
      </div>
      <p class="sync-hint">
        口令只保存在本机（Windows 加密），忘记后无法解开远端文件。
      </p>
      <p v-if="setupError" class="form-error">{{ setupError }}</p>
      <div class="modal-actions">
        <button
          type="button"
          class="secondary"
          :disabled="v.syncBusy"
          @click="setupDialog?.close()"
        >
          取消
        </button>
        <button
          type="button"
          class="secondary"
          :disabled="testing || v.syncBusy"
          @click="test"
        >
          {{ testing ? "测试中…" : "测试连接" }}
        </button>
        <button id="sync-save" class="primary" :disabled="v.syncBusy">
          {{ v.syncBusy ? "连接中…" : "保存并同步" }}
        </button>
      </div>
    </form>
  </dialog>

  <dialog ref="disableDialog" id="sync-disable-dialog">
    <h2>停用同步？</h2>
    <p class="dialog-description">
      本机不再自动同步，已保存的口令也会清除。本机和远端的数据都不会删除，以后可以用原口令重新加入。
    </p>
    <div class="modal-actions">
      <button class="secondary" autofocus @click="disableDialog?.close()">
        取消
      </button>
      <button
        class="primary danger-fill"
        :disabled="v.syncBusy"
        @click="disable"
      >
        停用同步
      </button>
    </div>
  </dialog>
</template>

<style scoped>
.sync-actions {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
  padding-top: 12px;
}
.sync-error {
  color: #9d4657;
}
.sync-inline {
  display: flex;
  gap: 8px;
}
.sync-inline input {
  flex: 1;
}
.sync-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 0 12px;
}
.sync-check {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 0 0 14px;
  color: #59657a;
  font-size: 12px;
}
.sync-check input {
  width: auto;
  margin: 0;
  accent-color: var(--accent);
}
.sync-hint {
  display: block;
  margin: 4px 0 12px;
  color: #687387;
  font-size: 11px;
  line-height: 1.7;
}
#sync-kind,
#sync-credential {
  width: 100%;
  border: 1px solid #dde2eb;
  border-radius: 6px;
  padding: 10px;
  background: #fcfdff;
  color: #344057;
  font: inherit;
}
</style>
