import type { Entry, ApiService } from "../types/models";
export const emptyService = (): ApiService => ({
  id: "",
  protocol: "custom",
  url: "",
});
const servicesFor = (entry?: Entry) => {
  if (!entry) return [emptyService()];
  if (entry.kind === "api" && entry.services.length === 0)
    return [emptyService()];
  return entry.services.map((service) => ({ ...service }));
};
export const fieldsFor = (entry?: Entry) => ({
  tags: [...(entry?.tags ?? [])],
  database: entry?.database
    ? { ...entry.database }
    : { databaseType: "MySQL", host: "", port: "3306", databaseName: "" },
  title: entry?.title ?? "",
  account: entry?.account ?? "",
  url: entry?.url ?? "",
  documentationUrl: entry?.documentationUrl ?? "",
  secret: "",
  notes: entry?.notes ?? "",
  services: servicesFor(entry),
});
