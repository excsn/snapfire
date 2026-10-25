<script setup lang="ts">
import { ref } from "vue";

const n = ref(0);
</script>

<template>
  <section class="frame" data-frame="vue">
    <button class="tick" @click="n = n + 1">{{ n }}</button>
    <slot />
  </section>
</template>
