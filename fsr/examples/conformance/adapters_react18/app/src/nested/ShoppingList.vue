<script setup lang="ts">
import { computed, ref } from "vue";

const asked = ref(false);
const label = computed(() => {
  if (asked.value) throw new Error("the shopping list is unavailable");
  return "Build the list";
});
</script>

<template>
  <button class="shopping" @click="asked = true">{{ label }}</button>
</template>
