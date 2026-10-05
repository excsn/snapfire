<script setup lang="ts">
import { ref } from "vue";

const props = defineProps<{ name: string; stock: number }>();
const taken = ref(0);
</script>

<template>
  <li class="tile">
    <span class="name">{{ props.name }}</span>
    <span class="left">{{ props.stock - taken }}</span>
    <slot />
    <button class="take" @click="taken++">take</button>
  </li>
</template>
