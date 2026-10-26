<script setup lang="ts">
import EntryList from "../components/EntryList.vue";
import EntryEditor from "../components/EntryEditor.vue";
import type { UnwrapRef } from "vue";
import type { VaultUI } from "../composables/useVault";
defineProps<{ v: UnwrapRef<VaultUI> }>();
defineEmits<{ organize: [kind: "tag", previous?: string] }>();
</script>
<template>
  <div class="split">
    <EntryList
      :v="v"
      @organize="(...args) => $emit('organize', ...args)"
    /><EntryEditor :v="v" />
  </div>
</template>
