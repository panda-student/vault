export const message = (value: unknown) => {
  if (!(value && typeof value === "object"))
    return "操作失败，请检查本地文件权限后重试。";
  const failure = value as { message?: unknown; details?: unknown };
  const base =
    typeof failure.message === "string" && failure.message
      ? failure.message
      : "操作失败，请检查本地文件权限后重试。";
  if (!Array.isArray(failure.details)) return base;
  const reasons = failure.details
    .map((issue) => {
      if (
        issue &&
        typeof issue === "object" &&
        "field" in issue &&
        "reason" in issue
      ) {
        const named = issue as { field: unknown; reason: unknown };
        return `${String(named.field)}：${String(named.reason)}`;
      }
      return "";
    })
    .filter(Boolean)
    .join("；");
  return reasons ? `${base}（${reasons}）` : base;
};
