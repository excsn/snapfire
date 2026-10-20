<script setup lang="ts">
const props = defineProps<{ label: string }>();
</script>

<template>
  <span class="fills">
    <Teleport to="#react-place"><p class="filled">{{ props.label }} in react</p></Teleport>
    <Teleport to="#vue-place"><p class="filled">{{ props.label }} in vue</p></Teleport>
    <Teleport to="#svelte-place"><p class="filled">{{ props.label }} in svelte</p></Teleport>
  </span>
</template>
