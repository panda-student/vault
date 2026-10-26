/** Copy JSON-serializable state so drafts never mutate persisted reactive objects. */
export function cloneJson<T>(value: T): T {
  return JSON.parse(JSON.stringify(value));
}
