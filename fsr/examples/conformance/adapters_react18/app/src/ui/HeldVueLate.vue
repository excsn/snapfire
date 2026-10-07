<script setup lang="ts">
import { onMounted, onUpdated } from "vue";
import { useStore } from "@snapfire/fsr-client/vue";

import { rendered } from "@src/probes";
import { probeCount } from "@src/store";

const count = useStore(probeCount, 0);
onMounted(() => rendered("vue-late"));
onUpdated(() => rendered("vue-late"));
</script>

<template>
  <p class="late">{{ count.value }}</p>
</template>
