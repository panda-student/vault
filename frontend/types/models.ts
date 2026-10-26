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
export interface Entry {
  id: string;
  kind: Kind;
  tags: string[];
  database: DatabaseFields | null;
  title: string;
  account: string;
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
}
export type PermissionKey = "access" | keyof McpPermissions;
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
