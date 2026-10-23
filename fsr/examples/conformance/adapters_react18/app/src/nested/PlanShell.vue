<script setup lang="ts">
import { provide, ref } from "vue";

const servings = ref(2);
provide("servings", servings);
</script>

<template>
  <div class="plan">
    <button class="plan-four" @click="servings = 4">Cook for four</button>
    <slot />
  </div>
</template>
