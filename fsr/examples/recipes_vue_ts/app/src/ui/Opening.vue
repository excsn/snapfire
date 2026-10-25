<script setup lang="ts">
import { useLinkStatus } from "@snapfire/fsr/client/vue";

const status = useLinkStatus();
</script>

<template>
  <span class="opening" :class="{ 'opening-on': status.pending }">{{ status.pending ? "opening…" : "" }}</span>
</template>
