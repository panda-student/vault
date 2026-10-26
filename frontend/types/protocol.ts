/** Method names mirror backend/src/api/rpc.rs. Caller identity is never a wire field. */
export type RpcMethod =
  | "application.info"
  | "credentials.list"
  | "credentials.search"
  | "credentials.get"
  | "credentials.create"
  | "credentials.update"
  | "credentials.delete"
  | "credentials.pin"
  | "credentials.readSecret"
  | "tags.list"
  | "tags.organize"
  | "trash.list"
  | "trash.restore"
  | "trash.empty"
  | "clipboard.copyField"
  | "clipboard.copyText"
  | "browser.open"
  | "settings.get"
  | "settings.save"
  | "permissions.preview"
  | "startup.set"
  | "updates.status"
  | "updates.check"
  | "updates.download"
  | "updates.cancel"
  | "updates.install"
  | "mcp.configuration"
  | "filling.write"
  | "filling.capture";
export interface ApiError {
  code: string;
  message: string;
  details?: { field: string; reason: string }[];
}
export interface RpcRequest {
  jsonrpc: "2.0";
  id: number;
  method: RpcMethod;
  params: unknown;
}
export type RpcResponse<T> =
  | { jsonrpc: "2.0"; id: number; result: T }
  | {
      jsonrpc: "2.0";
      id: number;
      error: { code: number; message: string; data?: ApiError };
    };
