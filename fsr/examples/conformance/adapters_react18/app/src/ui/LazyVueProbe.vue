<script setup lang="ts">
import { defineAsyncComponent } from "vue";
import { useStore } from "@snapfire/fsr/client/vue";

import { probeCount } from "@src/store";

const LazyVuePart = defineAsyncComponent(() => import("@src/ui/LazyVuePart.vue"));

const props = defineProps<{ label: string }>();
const count = useStore(probeCount, 0);
</script>

<template>
  <section class="lazy-probe" data-owner="vue">
    <h3 class="label">{{ props.label }}</h3>
    <p class="held">{{ count.value }}</p>
    <LazyVuePart />
  </section>
</template>
