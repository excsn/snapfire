<script setup lang="ts">
import { ref } from "vue";
import { get, optimistic } from "@snapfire/fsr-client";

import { actions } from "@generated/client";
import { plannedCount } from "@src/store";

const props = defineProps<{ id: string; planned: boolean }>();
const held = ref(props.planned);
const why = ref("");

async function toggle(): Promise<void> {
  const step = held.value ? -1 : 1;
  try {
    await optimistic(plannedCount, (get(plannedCount) ?? 0) + step, () =>
      held.value ? actions.recipe.$id.unplan({ recipe_id: props.id }) : actions.recipe.$id.plan({ recipe_id: props.id }),
    );
    held.value = !held.value;
    why.value = "";
  } catch (e) {
    why.value = e instanceof Error ? e.message : "that did not take";
  }
}
</script>

<template>
  <p class="plan">
    <button class="btn" :class="{ 'btn-on': held }" @click="toggle">{{ held ? "Planned for tonight" : "Cook this tonight" }}</button>
    <span v-if="why !== ''" class="why">{{ why }}</span>
  </p>
</template>

<style scoped>
.plan {
  display: flex;
  align-items: center;
  gap: 12px;
  margin: 0 0 20px;
}
.btn {
  border: 1px solid #1b1d21;
  border-radius: 6px;
  background: #fff;
  padding: 8px 16px;
  font: inherit;
  cursor: pointer;
}
.btn-on {
  background: #1b1d21;
  color: #fff;
}
.why {
  color: #b42318;
  font-size: 13px;
}
</style>
