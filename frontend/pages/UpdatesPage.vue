<script setup lang="ts">
import type { UnwrapRef } from "vue";
import type { VaultUI } from "../composables/useVault";
import UpdatePanel from "../components/UpdatePanel.vue";
defineProps<{ v: UnwrapRef<VaultUI> }>();
</script>

<template>
  <section class="updates-page" aria-label="软件更新">
    <UpdatePanel :v="v" />
  </section>
</template>
