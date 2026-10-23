<script setup lang="ts">
import { counted } from "@src/nested/stock";

const items = await counted;
</script>

<template>
  <span class="pantry-count">{{ items }} in the pantry</span>
</template>
