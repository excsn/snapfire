<script setup lang="ts">
import { computed, ref } from "vue";
import Search from "./Search.vue";
import Tile from "./Tile.vue";

const props = defineProps<{ items: { name: string; stock: number }[]; start?: string }>();
const query = ref(props.start ?? "");
const shown = computed(() => props.items.filter((item) => item.name.includes(query.value)));
</script>

<template>
  <Search v-model="query" />
  <ul class="shelf">
    <Tile v-for="item in shown" :key="item.name" :name="item.name" :stock="item.stock">
      <em class="note">{{ item.stock > 2 ? "plenty" : "low" }}</em>
    </Tile>
  </ul>
</template>

<style scoped>
.shelf {
  list-style: none;
}
</style>
