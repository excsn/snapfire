<script setup lang="ts">
import { ref } from "vue";
import { useStore } from "@snapfire/fsr-client/vue";

import HeldVueLate from "@src/ui/HeldVueLate.vue";
import { probeCount, probeOther } from "@src/store";

const props = defineProps<{ label: string }>();
const count = useStore(probeCount, 0);
const other = useStore(probeOther, 0);
const more = ref(false);
</script>

<template>
  <section class="held-probe" data-owner="vue">
    <h3 class="label">{{ props.label }}</h3>
    <p class="held">{{ count.value }}</p>
    <p class="other">{{ other.value }}</p>
    <button class="more" @click="more = true">more</button>
    <HeldVueLate v-if="more" />
  </section>
</template>
