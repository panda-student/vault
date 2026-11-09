import type { Entry, ApiService, GroupDraft } from "../types/models";
/** Whether the saved group already holds this secret (drafts never contain it). */
export const savedSecret = (
  group: Pick<GroupDraft, "id" | "hasPassword" | "hasToken">,
  field: "password" | "token",
) => !!group.id && (field === "password" ? group.hasPassword : group.hasToken);
export const emptyService = (): ApiService => ({
  id: "",
  protocol: "custom",
  url: "",
});
export const emptyGroup = (): GroupDraft => ({
  id: "",
  environment: "",
  account: "",
  password: "",
  token: "",
  hasPassword: false,
  hasToken: false,
});
const servicesFor = (entry?: Entry) => {
  if (!entry) return [emptyService()];
  if (entry.kind === "api" && entry.services.length === 0)
    return [emptyService()];
  return entry.services.map((service) => ({ ...service }));
};
const groupsFor = (entry?: Entry): GroupDraft[] =>
  entry?.groups.length
    ? entry.groups.map((group) => ({ ...group, password: "", token: "" }))
    : [emptyGroup()];
export const fieldsFor = (entry?: Entry) => ({
  tags: [...(entry?.tags ?? [])],
  database: entry?.database
    ? { ...entry.database }
    : { databaseType: "MySQL", host: "", port: "3306", databaseName: "" },
  // Only generic entries without a site need a name; others show their site.
  title: entry?.kind === "generic" ? entry.title : "",
  url: entry?.url ?? "",
  documentationUrl: entry?.documentationUrl ?? "",
  notes: entry?.notes ?? "",
  services: servicesFor(entry),
  groups: groupsFor(entry),
});
