<script setup lang="ts">
import { toRefs, ref } from "vue";
import type { UnwrapRef } from "vue";
import type { VaultUI } from "../composables/useVault";
import TransferPanel from "../components/TransferPanel.vue";
import thirdPartyNotices from "../../THIRD_PARTY_NOTICES.txt?raw";
const props = defineProps<{ v: UnwrapRef<VaultUI> }>();
const { info, settingsSaving } = toRefs(props.v);
const { updateSettings, toggleStartup } = props.v;
const licenses = ref<HTMLDialogElement | null>(null);
</script>
<template>
  <section class="settings">
    <div>
      <h2>按照你的习惯运行</h2>
      <p class="description">
        解锁随当前 Windows 身份完成，无需额外输入主密码。
      </p>
      <div class="card">
        <h3>启动与窗口</h3>
        <div class="setting-row">
          <div>
            登录 Windows 后启动<small
              >可选；移动程序后需重新开启以更新路径</small
            >
          </div>
          <button
            id="startup-toggle"
            class="switch"
            :class="{ off: !info?.startupEnabled }"
            role="switch"
            :aria-checked="info?.startupEnabled"
            aria-label="登录 Windows 后启动"
            @click="toggleStartup"
          ></button>
        </div>
        <div class="setting-row">
          <div>
            关闭窗口时保留在托盘<small>从托盘菜单可重新打开或彻底退出</small>
          </div>
          <button
            id="tray-toggle"
            class="switch"
            :class="{ off: !info?.settings.closeToTray }"
            role="switch"
            :aria-checked="info?.settings.closeToTray"
            aria-label="关闭窗口时保留在托盘"
            :disabled="settingsSaving || !info"
            @click="updateSettings('closeToTray')"
          ></button>
        </div>
      </div>
      <div class="card">
        <h3>本地密码库</h3>
        <div class="setting-row">
          <span>保护方式</span
          ><span class="pill">本地加密 · Windows 用户身份</span>
        </div>
        <div class="setting-row vertical">
          <span>数据目录</span
          ><code id="data-directory">{{ info?.dataDirectory }}</code
          ><small
            >请保留完整 data 目录。该密码库绑定创建它的 Windows
            账户与电脑。</small
          >
        </div>
        <div class="setting-row">
          <span>应用版本</span><span>{{ info?.version }}</span>
        </div>
        <div class="setting-row">
          <span>开源许可</span
          ><button class="text-btn" @click="licenses?.showModal()">
            查看第三方声明
          </button>
        </div>
      </div>
      <TransferPanel :v="v" />
    </div>
    <dialog ref="licenses" class="licenses-dialog">
      <h2>第三方开源声明</h2>
      <pre>{{ thirdPartyNotices }}</pre>
      <div class="modal-actions">
        <button class="secondary" autofocus @click="licenses?.close()">
          关闭
        </button>
      </div>
    </dialog>
  </section>
</template>

<style>
.licenses-dialog {
  width: min(800px, 85vw);
}
.licenses-dialog pre {
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  max-height: 50vh;
  overflow: auto;
  font-size: 11px;
}
</style>
