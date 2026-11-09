import type {
  CredentialRef,
  SyncKind,
  SyncStatus,
  SyncTarget,
} from "../types/sync";

export const DEFAULT_SYNC_PATH = "vault/vault-sync.vaultsync";

function sameCredential(first: CredentialRef, second: CredentialRef) {
  return (
    first.credentialId === second.credentialId &&
    first.groupId === second.groupId
  );
}

export function sameSyncTarget(first: SyncTarget, second: SyncTarget) {
  if ((first.path ?? DEFAULT_SYNC_PATH) !== (second.path ?? DEFAULT_SYNC_PATH))
    return false;

  switch (first.kind) {
    case "folder":
      return second.kind === "folder" && first.directory === second.directory;
    case "webdav":
      return (
        second.kind === "webdav" &&
        first.url === second.url &&
        sameCredential(first.credential, second.credential)
      );
    case "github":
      return (
        second.kind === "github" &&
        first.owner === second.owner &&
        first.repo === second.repo &&
        first.branch === second.branch &&
        sameCredential(first.token, second.token)
      );
  }
}

const GITHUB_NAME = /^[A-Za-z0-9_.-]+$/;

/**
 * Owner, repo and optional branch from a repository address, such as
 * `https://github.com/owner/repo`, `github.com/owner/repo.git` or
 * `https://github.com/owner/repo/tree/main`. Null when it does not parse.
 */
export function parseGitHubRepo(
  input: string,
): { owner: string; repo: string; branch?: string } | null {
  const text = input
    .trim()
    .replace(/^(https?:\/\/)?(www\.)?github\.com\//i, "")
    .replace(/[?#].*$/, "")
    .replace(/\/+$/, "");
  const [owner, name, ...rest] = text.split("/").filter(Boolean);
  if (!owner || !name) return null;
  const repo = name.replace(/\.git$/i, "");
  if (!GITHUB_NAME.test(owner) || !GITHUB_NAME.test(repo)) return null;
  if (rest.length === 0) return { owner, repo };
  if (rest[0] === "tree" && rest.length > 1)
    return { owner, repo, branch: rest.slice(1).join("/") };
  return null;
}

/** The address a saved GitHub target was set up from. */
export function githubAddress(target: {
  owner: string;
  repo: string;
  branch?: string;
}) {
  return `https://github.com/${target.owner}/${target.repo}${
    target.branch ? `/tree/${target.branch}` : ""
  }`;
}

export const syncKinds: Record<SyncKind, string> = {
  folder: "文件夹 / 网盘",
  webdav: "WebDAV",
  github: "GitHub 私有仓库",
};

export function syncTime(value: number | null | undefined) {
  return value
    ? new Date(value).toLocaleString("zh-CN", { hour12: false })
    : "—";
}

/** One line for the settings card and the sidebar. */
export function syncSummary(status: SyncStatus | null) {
  if (!status?.configured) return "未设置";
  if (status.running) return "同步中…";
  if (status.error) return `同步失败：${status.error.message}`;
  if (status.conflicts.length) return `${status.conflicts.length} 条冲突待处理`;
  if (!status.lastSyncAt) return "尚未同步";
  const { pushed, pulled, deleted } = status.lastOutcome;
  const changes = [
    pushed && `上传 ${pushed}`,
    pulled && `下载 ${pulled}`,
    deleted && `删除 ${deleted}`,
  ].filter(Boolean);
  return `${syncTime(status.lastSyncAt)} 已同步${changes.length ? `（${changes.join("，")}）` : ""}`;
}
