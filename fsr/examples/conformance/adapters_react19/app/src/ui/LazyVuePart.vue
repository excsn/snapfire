<script setup lang="ts">
import { onMounted, onUpdated } from "vue";
import { useStore } from "@snapfire/fsr-client/vue";

import { rendered } from "@src/probes";
import { probeCount } from "@src/store";

const count = useStore(probeCount, 0);
onMounted(() => rendered("vue-lazy"));
onUpdated(() => rendered("vue-lazy"));
</script>

<template>
  <p class="lazy-part">{{ count.value }}</p>
</template>
