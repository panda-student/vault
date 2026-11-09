import type { GroupSummary, Kind } from "./models";

export type SyncKind = "folder" | "webdav" | "github";
export type SyncChoice = "localWins" | "remoteWins" | "twoWay";

export interface CredentialRef {
  credentialId: string;
  groupId?: string;
}

export type SyncTarget =
  | { kind: "folder"; directory: string; path?: string }
  | { kind: "webdav"; url: string; path?: string; credential: CredentialRef }
  | {
      kind: "github";
      owner: string;
      repo: string;
      branch?: string;
      path?: string;
      token: CredentialRef;
    };

export interface SyncOutcome {
  pushed: number;
  pulled: number;
  deleted: number;
  conflicts: number;
}

export interface SyncConflict {
  id: string;
  name: string;
  localChangedAt: number | null;
  remoteChangedAt: number | null;
  localDevice: string;
  remoteDevice: string;
}

export interface SyncStatus {
  configured: boolean;
  target?: SyncTarget;
  location?: string;
  credentialName?: string;
  intervalMinutes: number;
  deviceName: string;
  running: boolean;
  lastSyncAt?: number;
  lastOutcome: SyncOutcome;
  error?: { code: string; message: string };
  conflicts: SyncConflict[];
  firstJoin: boolean;
}

export interface SyncSetup {
  target: SyncTarget;
  passphrase?: string;
  intervalMinutes?: number;
  deviceName?: string;
  create: boolean;
}

/** A vault entry whose group can supply a WebDAV password or GitHub token. */
export interface SyncCredential {
  id: string;
  name: string;
  kind: Kind;
  groups: GroupSummary[];
}

/** A sync location the AI asked for, waiting for the user's passphrase. */
export interface PendingSync {
  id: string;
  kind: SyncKind;
  location: string;
  credentialName?: string;
  create: boolean;
  requestedAt: number;
}
