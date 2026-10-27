import type { Kind } from "./models";
export type TransferFormat = "json" | "csv";
export interface TransferInspection {
  encrypted: boolean;
  format: TransferFormat;
  count?: number;
}
export interface TransferExportOptions {
  format: TransferFormat;
  includeTrash: boolean;
  kinds: Kind[];
  encrypted: boolean;
  passphrase: string | null;
  path: string;
}
export interface ExportResult {
  count: number;
  path: string;
}
export interface ImportResult {
  imported: number;
}
export interface PendingExport {
  id: string;
  count: number;
  includeTrash: boolean;
  requestedAt: number;
}
