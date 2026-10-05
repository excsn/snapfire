<script setup lang="ts">
import { useStore } from "@snapfire/fsr-client/vue";

import { owner } from "@src/store";

const props = defineProps<{ name: string }>();
const value = useStore(owner, "none");
</script>

<template>
  <p class="owner" :data-island="props.name">{{ value.value }}</p>
</template>
