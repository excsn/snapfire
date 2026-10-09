<script setup lang="ts">
import { inject } from "vue";

import { scaleKey } from "@src/ui/scale";

defineProps<{ quantity: number; unit: string }>();
const scale = inject(scaleKey, 1);
</script>

<template>
  <span class="quantity">{{ Math.round(quantity * scale * 100) / 100 }}{{ unit ? " " + unit : "" }}</span>
</template>

<style scoped>
.quantity {
  font-variant-numeric: tabular-nums;
}
</style>
