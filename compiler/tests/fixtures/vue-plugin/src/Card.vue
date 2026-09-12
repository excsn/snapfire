<script setup lang="ts">
import { ref } from "vue";
import { shout } from "./util";
const props = defineProps<{ title: string }>();
const n = ref(0);
</script>

<template>
  <div class="card">
    <h2>{{ shout(title) }}</h2>
    <button @click="n++">clicked {{ n }}</button>
  </div>
</template>

<style scoped>
.card { padding: 12px; }
</style>
