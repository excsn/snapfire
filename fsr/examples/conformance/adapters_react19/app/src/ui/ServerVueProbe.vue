<script setup lang="ts">
import { useStore } from "@snapfire/fsr-client/vue";

import { probeCount } from "@src/store";

const count = useStore(probeCount, 0);
function add() {
  count.value = count.value + 1;
}
</script>

<template>
  <section class="probe" data-owner="server-vue">
    <h3 class="label">server vue</h3>
    <p class="count">{{ count.value }}</p>
    <button class="add" @click="add">add</button>
  </section>
</template>
