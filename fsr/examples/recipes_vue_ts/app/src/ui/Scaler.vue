<script setup lang="ts">
import { computed, ref } from "vue";

interface Ingredient {
  name: string;
  quantity: number;
  unit: string;
}

const props = defineProps<{ serves: number; ingredients: Ingredient[] }>();
const wanted = ref(props.serves);

const scaled = computed(() =>
  props.ingredients.map((i) => ({
    ...i,
    quantity: Math.round((i.quantity * wanted.value * 100) / props.serves) / 100,
  })),
);

function fewer(): void {
  if (wanted.value > 1) wanted.value -= 1;
}
function more(): void {
  wanted.value += 1;
}
</script>

<template>
  <div class="scaler">
    <h3>Ingredients</h3>
    <p class="serves">
      <button class="step" aria-label="fewer" @click="fewer">−</button>
      <span class="count">serves {{ wanted }}</span>
      <button class="step" aria-label="more" @click="more">+</button>
      <span v-if="wanted !== serves" class="scaled-note">scaled from {{ serves }}</span>
    </p>
    <ul>
      <li v-for="i in scaled" :key="i.name">
        <span class="quantity">{{ i.quantity }}{{ i.unit ? " " + i.unit : "" }}</span>
        <span class="name">{{ i.name }}</span>
      </li>
    </ul>
  </div>
</template>

<style scoped>
.scaler {
  border-top: 1px solid #e3e5ea;
  padding-top: 16px;
}
.scaler h3 {
  margin: 0 0 8px;
  font-size: 16px;
}
.serves {
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 0 0 12px;
}
.step {
  width: 30px;
  height: 30px;
  border: 1px solid #1b1d21;
  border-radius: 50%;
  background: #fff;
  font: inherit;
  cursor: pointer;
}
.scaled-note {
  color: #6b7280;
  font-size: 13px;
}
ul {
  margin: 0;
  padding: 0;
  list-style: none;
}
li {
  display: grid;
  grid-template-columns: 110px 1fr;
  padding: 4px 0;
  border-bottom: 1px dotted #e3e5ea;
}
.quantity {
  font-variant-numeric: tabular-nums;
}
</style>
