import type { Kind, ApiProtocol, Entry, GroupSummary } from "../types/models";
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
/** Entries are shown by the name the backend derives from their site. */
export const entryName = (entry: Pick<Entry, "name" | "title">) =>
  entry.name || entry.title || "未命名";
export const groupLabel = (
  group: Pick<GroupSummary, "account" | "environment">,
) =>
  group.account && group.environment
    ? `${group.account}（${group.environment}）`
    : group.account || group.environment;
/** Second line of a list item: every account of the entry with its environment. */
export const accountsLine = (entry: Pick<Entry, "kind" | "groups">) =>
  entry.groups.map(groupLabel).filter(Boolean).join(" · ") ||
  kindName(entry.kind);
export const timestamp = (value: number) =>
  new Date(value).toLocaleString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
