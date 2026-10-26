import { invoke as tauriInvoke } from "@tauri-apps/api/core";
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
