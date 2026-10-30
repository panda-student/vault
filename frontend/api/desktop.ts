import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { downloadDir, join } from "@tauri-apps/api/path";
import { open, save } from "@tauri-apps/plugin-dialog";
import type {
  RpcRequest,
  RpcResponse,
  ApiError,
  RpcMethod,
} from "../types/protocol";
let serial = 0;
/** Send a JSON-RPC request; only this module knows the Tauri command name. */
export async function request<T>(
  method: RpcMethod,
  params: unknown = {},
): Promise<T> {
  const id = ++serial;
  const envelope: RpcRequest = { jsonrpc: "2.0", id, method, params };
  const reply = await tauriInvoke<RpcResponse<T>>("rpc", { request: envelope });
  if (!reply || reply.jsonrpc !== "2.0" || reply.id !== id)
    throw {
      code: "RPC_PROTOCOL",
      message: "后端响应格式不正确。",
    } satisfies ApiError;
  if ("error" in reply)
    throw (
      reply.error.data ?? {
        code: String(reply.error.code),
        message: reply.error.message,
      }
    );
  return reply.result;
}
export const invoke = request;
/** Only file selection uses the OS dialog; every other surface stays in Vue. */
export async function pickExportPath(name: string): Promise<string | null> {
  let defaultPath = name;
  try {
    defaultPath = await join(await downloadDir(), name);
  } catch {
    /* Fall back to the dialog's remembered folder. */
  }
  const extension = name.slice(name.lastIndexOf(".") + 1);
  return save({
    defaultPath,
    filters: [{ name: "凭据导出文件", extensions: [extension] }],
  });
}
/** Brings the window forward while an AI export waits for the user's passphrase. */
export function setExportAttention(active: boolean): Promise<void> {
  return tauriInvoke<void>("export_attention", { active });
}
export async function pickImportPath(): Promise<string | null> {
  const selected = await open({
    multiple: false,
    directory: false,
    filters: [{ name: "凭据文件", extensions: ["json", "csv", "vaultenc"] }],
  });
  return typeof selected === "string" ? selected : null;
}
