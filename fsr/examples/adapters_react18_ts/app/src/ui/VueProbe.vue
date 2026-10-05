<script setup lang="ts">
import { onMounted, onUnmounted, onUpdated } from "vue";
import { Link, Mount, Picture, useLocale, useStore } from "@snapfire/fsr-client/vue";

import { rendered, unmounted } from "@src/probes";
import { probeCount } from "@src/store";

const props = defineProps<{ label: string; nest: string[] }>();
const count = useStore(probeCount, 0);
const locale = useLocale();
onMounted(() => rendered("vue"));
onUpdated(() => rendered("vue"));
onUnmounted(() => unmounted("vue"));
</script>

<template>
  <section class="probe" data-owner="vue">
    <h3 class="label">{{ props.label }}</h3>
    <p class="count">{{ count.value }}</p>
    <button class="add" @click="count.value = count.value + 1">add</button>
    <p class="locale">{{ locale }}</p>
    <Link href="/next" class="next">next</Link>
    <Picture src="/pictures/probe.png" alt="probe" :width="4" :height="4" />
    <div class="children"><slot /></div>
    <Mount v-for="module in props.nest" :key="module" :module="module" :props="{ label: props.label + ' nested', nest: [] }" />
  </section>
</template>
