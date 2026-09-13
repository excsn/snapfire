<script setup lang="ts">
const props = defineProps<{ points: number[]; up: boolean }>();

const path = (): string => {
  const points = props.points;
  const low = Math.min(...points);
  const high = Math.max(...points);
  const span = high - low || 1;
  return points
    .map((p, i) => `${(i / (points.length - 1)) * 60} ${16 - ((p - low) / span) * 14}`)
    .map((pair, i) => `${i === 0 ? "M" : "L"}${pair}`)
    .join(" ");
};
</script>

<template>
  <svg class="spark" viewBox="0 0 60 18" width="60" height="18" aria-hidden="true">
    <path :d="path()" fill="none" :stroke="props.up ? '#15803d' : '#b91c1c'" stroke-width="1.5" />
  </svg>
</template>

<style scoped>
.spark { display: block; }
</style>
