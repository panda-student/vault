import { invoke } from "../api/desktop";
import type { Settings } from "../types/models";
import type { VaultState } from "./state";
import type { VaultDependencies } from "./dependencies";
import { message } from "../utils/errors";
export function useSettings(
  state: Pick<VaultState, "info" | "settingsSaving" | "config">,
  dependencies: Pick<VaultDependencies, "notify">,
) {
  const { info, settingsSaving, config } = state;
  const { notify } = dependencies;

  async function persistSettings(settings: Settings) {
    if (!info.value || settingsSaving.value) return false;
    settingsSaving.value = true;
    try {
      info.value.settings = await invoke<Settings>("settings.save", {
        settings,
      });
      notify("设置已保存");
      return true;
    } catch (value) {
      notify(message(value));
      return false;
    } finally {
      settingsSaving.value = false;
    }
  }
  async function updateSettings(key: "mcpEnabled" | "closeToTray") {
    if (info.value)
      await persistSettings({
        ...info.value.settings,
        [key]: !info.value.settings[key],
      });
  }
  async function savePermissions(settings: Settings) {
    return info.value
      ? persistSettings({
          ...settings,
          mcpEnabled: info.value.settings.mcpEnabled,
          closeToTray: info.value.settings.closeToTray,
        })
      : false;
  }
  async function toggleStartup() {
    if (!info.value) return;
    try {
      info.value.startupEnabled = await invoke<boolean>("startup.set", {
        enabled: !info.value.startupEnabled,
      });
      notify("启动设置已更新");
    } catch (value) {
      notify(message(value));
    }
  }
  async function copyConfig() {
    try {
      await invoke("clipboard.copyText", { text: config.value });
      notify("已复制 MCP 配置");
    } catch (value) {
      notify(message(value));
    }
  }
  return {
    persistSettings,
    updateSettings,
    savePermissions,
    toggleStartup,
    copyConfig,
  };
}
