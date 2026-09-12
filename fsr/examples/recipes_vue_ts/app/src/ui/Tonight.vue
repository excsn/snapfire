<script setup lang="ts">
import { ref } from "vue";
import { useStore } from "@snapfire/fsr-client/vue";

import { plannedCount } from "@src/store";

const props = defineProps<{ count: number }>();
const held = useStore(plannedCount, props.count);
const open = ref(false);
</script>

<template>
  <div class="tonight" :class="{ 'tonight-open': open }">
    <button class="tonight-count" aria-label="tonight" @click="open = !open">{{ held.value }} for tonight</button>
    <p v-if="open" class="tonight-note">
      Kept in the session cookie. <a href="/tonight">See them</a>. This panel is Vue state in the root layout: move between recipes and it stays open.
    </p>
  </div>
</template>

<style scoped>
.tonight {
  position: relative;
  margin-left: 16px;
}
.tonight-count {
  border: 1px solid #1b1d21;
  border-radius: 999px;
  background: #fff;
  padding: 6px 14px;
  font: inherit;
  cursor: pointer;
}
.tonight-open .tonight-count {
  background: #1b1d21;
  color: #fff;
}
.tonight-note {
  position: absolute;
  right: 0;
  top: 44px;
  width: 280px;
  margin: 0;
  padding: 12px 14px;
  background: #fff;
  border: 1px solid #e3e5ea;
  border-radius: 8px;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.08);
  font-size: 13px;
  z-index: 2;
}
</style>
