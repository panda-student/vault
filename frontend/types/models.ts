export type Kind = "password" | "api" | "database" | "generic";
export interface DatabaseFields {
  databaseType: string;
  host: string;
  port: string;
  databaseName: string;
}
export interface Organization {
  tags: string[];
}
export interface TrashEntry extends Entry {
  deletedAt: number;
}
export type ApiProtocol = "openai" | "anthropic" | "gemini" | "custom";
export interface ApiService {
  id: string;
  protocol: ApiProtocol;
  url: string;
}
/** One login of an entry; secrets never leave the backend in listings. */
export interface GroupSummary {
  id: string;
  environment: string;
  account: string;
  hasPassword: boolean;
  hasToken: boolean;
}
/** Editable group: password/token hold a newly typed value; empty keeps the saved one. */
export interface GroupDraft extends GroupSummary {
  password: string;
  token: string;
}
export type GroupField = "account" | "password" | "token";
export interface Entry {
  id: string;
  kind: Kind;
  tags: string[];
  database: DatabaseFields | null;
  /** Stored title; only entries without a website are shown by it. */
  title: string;
  /** Display name from the backend: the website host, else the title. */
  name: string;
  groups: GroupSummary[];
  url: string;
  documentationUrl: string;
  services: ApiService[];
  notes: string;
  pinned: boolean;
  createdAt: number;
  updatedAt: number;
  revision: number;
}
export interface McpPermissions {
  readMetadata: boolean;
  create: boolean;
  update: boolean;
  delete: boolean;
  fillInput: boolean;
  writeFile: boolean;
  /** Global only: read all settings and change non-permission settings. */
  settings: boolean;
}
export type PermissionKey =
  | "access"
  | Exclude<keyof McpPermissions, "settings">;
export type PermissionRule = Partial<Record<PermissionKey, boolean>>;
export interface Settings {
  mcpEnabled: boolean;
  closeToTray: boolean;
  mcpPermissions: McpPermissions;
  mcpAccess: boolean;
  mcpTagRules: Record<string, PermissionRule>;
  mcpEntryRules: Record<string, PermissionRule>;
}
export interface Info {
  backendPid: number;
  version: string;
  executable: string;
  dataDirectory: string;
  webviewVersion: string;
  smokeTest: boolean;
  settings: Settings;
  startupEnabled: boolean;
}
