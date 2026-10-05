<script setup lang="ts">
import Tile from "./Tile.vue";

const props = defineProps<{ items: { name: string; stock: number }[] }>();
</script>

<template>
  <ul class="shelf">
    <Tile v-for="item in props.items" :key="item.name" :name="item.name" :stock="item.stock">
      <em class="note">{{ item.stock > 2 ? "plenty" : "low" }}</em>
    </Tile>
  </ul>
</template>

<style scoped>
.shelf {
  list-style: none;
}
</style>
