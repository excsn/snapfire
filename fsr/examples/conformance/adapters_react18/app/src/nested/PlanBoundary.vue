<script setup lang="ts">
import { onErrorCaptured, ref } from "vue";

const failed = ref(false);
onErrorCaptured(() => {
  failed.value = true;
  return false;
});
</script>

<template>
  <div v-if="failed" class="plan-failed">
    The plan hit a problem.
    <button class="plan-retry" @click="failed = false">Try again</button>
  </div>
  <Suspense v-else :timeout="0">
    <div class="plan-body"><slot /></div>
    <template #fallback><p class="plan-loading">Loading the plan</p></template>
  </Suspense>
</template>
