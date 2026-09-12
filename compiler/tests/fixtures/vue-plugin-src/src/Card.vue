<script setup lang="ts">
const props = defineProps<{ title: string }>();
</script>

<template>
  <div class="card">{{ title }}</div>
</template>

<style src="./card.css" scoped></style>
