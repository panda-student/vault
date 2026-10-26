import type { Entry, ApiProtocol } from "../types/models";
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
  services: entry
    ? entry.services.map((service) => ({ ...service }))
    : [{ id: "", protocol: "openai" as ApiProtocol, url: "" }],
});
