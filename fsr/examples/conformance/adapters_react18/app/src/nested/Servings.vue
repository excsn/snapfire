<script setup lang="ts">
import { inject, type Ref } from "vue";

const servings = inject<Ref<number>>("servings");
</script>

<template>
  <p class="servings">Serves {{ servings ?? 1 }}</p>
</template>
