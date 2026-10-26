import type { Entry } from "../types/models";
export interface VaultDependencies {
  notify(value: string): void;
  hide(): void;
  invalidateReveal(): void;
  refresh(): Promise<void>;
  stashDraft(): void;
  loadEntry(entry?: Entry, restore?: boolean): void;
  draftKey(): string | null | undefined;
  reconcile(entry: Entry): void;
  navigate(page: "entries" | "trash" | "settings" | "mcp"): void;
}
