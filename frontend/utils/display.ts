import type { Kind, ApiProtocol, Entry } from "../types/models";
export const missingSite = (entry: Pick<Entry, "kind" | "url">) =>
  (entry.kind === "password" || entry.kind === "api") && !entry.url.trim();
export const protocolName = (protocol: ApiProtocol) =>
  ({
    openai: "OpenAI 兼容",
    anthropic: "Anthropic",
    gemini: "Gemini",
    custom: "HTTP/HTTPS",
  })[protocol];
export const kindName = (kind: Kind) =>
  ({
    password: "账号密码",
    api: "API 凭据",
    database: "数据库",
    generic: "通用",
  })[kind];
export const secretName = (kind: Kind) =>
  kind === "api" ? "API Key / Token" : "密码";
export const timestamp = (value: number) =>
  new Date(value).toLocaleString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
