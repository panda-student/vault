export interface UpdateStatus {
  phase:
    | "idle"
    | "checking"
    | "available"
    | "downloading"
    | "ready"
    | "installing"
    | "up-to-date"
    | "error";
  version: string | null;
  notes: string;
  received: number;
  total: number;
  error: string | null;
}
