<script setup lang="ts">
import { ref } from "vue";

defineProps<{ title: string; method: string }>();
const open = ref(false);
</script>

<template>
  <button class="cook-mode" @click="open = true">Cook mode</button>
  <Teleport to="body">
    <dialog class="cook" :open="open">
      <h2>{{ title }}</h2>
      <p>{{ method }}</p>
      <button class="done" @click="open = false">Done</button>
    </dialog>
  </Teleport>
</template>

<style scoped>
.cook-mode {
  border: 1px solid #1b1d21;
  border-radius: 6px;
  background: #fff;
  padding: 6px 12px;
  font: inherit;
  cursor: pointer;
}
.cook {
  position: fixed;
  inset: 0;
  width: 100%;
  height: 100%;
  margin: 0;
  border: 0;
  padding: 48px;
  box-sizing: border-box;
  background: #fffdf7;
  font-size: 22px;
}
.cook h2 {
  margin: 0 0 16px;
}
.done {
  border: 1px solid #1b1d21;
  border-radius: 6px;
  background: #fff;
  padding: 8px 16px;
  font: inherit;
  cursor: pointer;
}
</style>
