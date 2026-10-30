import { invoke } from "../api/desktop";
import type { Info } from "../types/models";
import type { VaultState } from "./state";
import type { VaultDependencies } from "./dependencies";
export function useTags(
  state: Pick<
    VaultState,
    | "page"
    | "tagFilter"
    | "typeFilter"
    | "missingOnly"
    | "info"
    | "permissionDraft"
    | "editing"
    | "form"
    | "baseline"
    | "drafts"
  >,
  dependencies: Pick<VaultDependencies, "refresh" | "stashDraft" | "navigate">,
) {
  const {
    page,
    tagFilter,
    typeFilter,
    missingOnly,

    info,
    permissionDraft,
    editing,
    form,
    baseline,
    drafts,
  } = state;
  const { refresh, stashDraft, navigate } = dependencies;

  async function organize(kind: "tag", name: string, previous?: string) {
    await invoke("tags.organize", { kind, name, previous });
    if (previous) {
      const rename = (tags: string[]) => [
        ...new Set(
          tags
            .filter((t) => t !== previous)
            .concat(tags.includes(previous) && name ? [name] : []),
        ),
      ];
      for (const draft of drafts.values()) {
        draft.fields.tags = rename(draft.fields.tags);
        if (draft.entry) draft.entry.tags = rename(draft.entry.tags);
        const saved = JSON.parse(draft.baseline);
        saved.tags = rename(saved.tags);
        draft.baseline = JSON.stringify(saved);
      }
      const oldBaseline = JSON.parse(baseline.value || "{}");
      form.value.tags = rename(form.value.tags);
      if (oldBaseline.tags) {
        oldBaseline.tags = rename(oldBaseline.tags);
        baseline.value = JSON.stringify(oldBaseline);
      }
      if (editing.value) editing.value.tags = rename(editing.value.tags);
      if (permissionDraft.value?.mcpTagRules[previous]) {
        const rule = permissionDraft.value.mcpTagRules[previous];
        delete permissionDraft.value.mcpTagRules[previous];
        if (name) permissionDraft.value.mcpTagRules[name] = rule;
      }
    }
    if (tagFilter.value === previous) tagFilter.value = name;
    await refresh();
    if (info.value)
      info.value.settings = (await invoke<Info>("application.info")).settings;
  }
  function filterTag(tag: string) {
    stashDraft();
    page.value = "entries";
    tagFilter.value = tagFilter.value === tag ? "" : tag;
  }
  function allEntries() {
    tagFilter.value = "";
    typeFilter.value = "";
    missingOnly.value = false;
    navigate("entries");
  }
  return { organize, filterTag, allEntries };
}
