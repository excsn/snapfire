<script setup lang="ts">
import { computed, inject, ref } from "vue";

const props = defineProps<{ name: string; stock: number }>();
const taken = ref(0);
const shelf = inject("shelf", "nowhere");
const left = computed(() => props.stock - taken.value);
const marks = computed(() => ({ "data-name": props.name, "data-left": left.value }));
const flag = computed(() => (left.value > 2 ? "data-plenty" : "data-low"));
</script>

<template>
  <li class="tile" v-bind="marks" :[flag]="shelf">
    <span class="name">{{ props.name }}</span>
    <span class="left">{{ left }}</span>
    <span class="shelf">{{ shelf }}</span>
    <slot />
    <slot name="badge" :left="left">none</slot>
    <button class="take" @click="taken++">take</button>
  </li>
</template>

<style scoped>
.tile {
  list-style: none;
}
:slotted(.note) {
  font-style: normal;
}
</style>
