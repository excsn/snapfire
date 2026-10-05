<script setup lang="ts">
const query = defineModel<string>({ default: "" });
</script>

<template>
  <label class="search">find <input class="query" v-model="query" /></label>
</template>
