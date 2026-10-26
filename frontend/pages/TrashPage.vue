<script setup lang="ts">
import TrashPanel from "../components/TrashPanel.vue";
import type { UnwrapRef } from "vue";
import type { VaultUI } from "../composables/useVault";
defineProps<{ v: UnwrapRef<VaultUI> }>();
</script>
<template><TrashPanel :v="v" /></template>
