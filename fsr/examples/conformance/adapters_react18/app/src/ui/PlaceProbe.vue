<script setup lang="ts">
const props = defineProps<{ label: string }>();
</script>

<template>
  <span class="place-probe">
    {{ props.label }}
    <Teleport to="#probe-place"><p class="placed">{{ props.label }}</p></Teleport>
  </span>
</template>
