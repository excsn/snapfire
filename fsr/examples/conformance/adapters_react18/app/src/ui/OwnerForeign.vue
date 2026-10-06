<script setup lang="ts">
import { inject } from "vue";
import { useStore } from "@snapfire/fsr-client/vue";

import { owner } from "@src/store";

const props = defineProps<{ name: string }>();
const tone = inject("tone", () => "plain", true);
const value = useStore(owner, "none");
</script>

<template>
  <p class="owner-unrendered" :data-island="props.name" :data-tone="tone">{{ value.value }}</p>
</template>
