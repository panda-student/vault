<script setup lang="ts">
import { toRefs, ref } from "vue";
import type { UnwrapRef } from "vue";
import type { VaultUI } from "../composables/useVault";
import PermissionsPanel from "../components/PermissionsPanel.vue";
const props = defineProps<{ v: UnwrapRef<VaultUI> }>();
const { info, configVisible, config, settingsSaving } = toRefs(props.v);
const { updateSettings, copyConfig } = props.v;
const tab = ref<"connection" | "permissions">("permissions");
</script>
<template>
  <section class="settings mcp-settings">
    <div>
      <div class="setting-row" style="padding-top: 0">
        <div>
          允许 MCP 访问<small>关闭后，已连接客户端的后续请求也会被拒绝</small>
        </div>
        <button
          id="mcp-toggle"
          class="switch"
          :class="{ off: !info?.settings.mcpEnabled }"
          role="switch"
          :aria-checked="info?.settings.mcpEnabled"
          aria-label="允许 MCP 访问"
          :disabled="settingsSaving || !info"
          @click="updateSettings('mcpEnabled')"
        ></button>
      </div>
      <nav class="mcp-tabs" role="tablist" aria-label="MCP 管理">
        <button
          id="connection-tab"
          role="tab"
          :aria-selected="tab === 'connection'"
          :class="{ active: tab === 'connection' }"
          @click="tab = 'connection'"
        >
          连接配置</button
        ><button
          id="permissions-tab"
          role="tab"
          :aria-selected="tab === 'permissions'"
          :class="{ active: tab === 'permissions' }"
          @click="tab = 'permissions'"
        >
          权限管理
        </button>
      </nav>
      <div v-show="tab === 'connection'">
        <h2>把凭据库连接到 AI</h2>
        <p class="description">
          多个 AI
          客户端共用后台服务，桌面窗口关闭后也可以使用。更新时服务自动暂停并恢复。
        </p>
        <div class="card">
          <h3>MCP 访问</h3>
          <div class="setting-row">
            <div>连接方式<small>客户端启动本地程序，不监听网络端口</small></div>
            <span class="pill">stdio · 共享服务</span>
          </div>
          <div class="setting-row">
            <div>客户端配置<small>路径已按当前程序位置生成</small></div>
            <button
              id="show-config"
              class="secondary"
              @click="configVisible = !configVisible"
            >
              {{ configVisible ? "收起" : "查看配置" }}
            </button>
          </div>
          <div v-if="configVisible">
            <pre id="mcp-config">{{ config }}</pre>
            <button id="copy-config" class="primary" @click="copyConfig">
              复制配置
            </button>
          </div>
        </div>
        <p class="description">
          本软件的 AI
          输出不返回凭据原文，备注默认不外发。凭据填写到目标后，其他程序仍可能读取目标内容。输入事件已发送不代表填写或登录成功。
        </p>
      </div>
      <PermissionsPanel v-show="tab === 'permissions'" :v="v" />
    </div>
  </section>
</template>
<style>
.settings.mcp-settings {
  max-width: none;
  min-width: 0;
}
.mcp-tabs {
  display: flex;
  gap: 26px;
  border-bottom: 1px solid #e7eaf1;
  margin: 6px 0 25px;
}
.mcp-tabs button {
  position: relative;
  background: none;
  border: 0;
  font: inherit;
  font-size: 12px;
  color: #8994a9;
  padding: 14px 0;
}
.mcp-tabs button.active {
  color: #5364dc;
  font-weight: 600;
}
.mcp-tabs button.active:after {
  content: "";
  position: absolute;
  left: 0;
  right: 0;
  bottom: -1px;
  height: 2px;
  background: #5364dc;
}
</style>
