<script setup lang="ts">
import { Mount, useStore } from "@snapfire/fsr-client/vue";

import { watchedKey } from "./Watch.js";

interface Holding {
  symbol: string;
  name: string;
  shares: bigint;
  price: number;
  change: number;
}

const props = defineProps<{ holdings: Holding[]; watched: string }>();
const held = useStore(watchedKey, props.watched);
</script>

<template>
  <table class="holdings">
    <thead>
      <tr><th>Symbol</th><th>Name</th><th class="n">Shares</th><th class="n">Price</th><th class="n">Day</th></tr>
    </thead>
    <tbody>
      <tr v-for="row in props.holdings" :key="row.symbol" :class="{ held: row.symbol === held.value }" @click="held.value = row.symbol">
        <td class="sym">{{ row.symbol }}</td>
        <td>{{ row.name }}</td>
        <td class="n">{{ row.shares }}</td>
        <td class="n">{{ row.price.toFixed(2) }}</td>
        <td class="n"><Mount module="js/src/ui/Chip.tsx#default" :props="{ symbol: row.symbol, change: row.change }" /></td>
      </tr>
    </tbody>
  </table>
</template>

<style scoped>
.holdings { width: 100%; border-collapse: collapse; }
.holdings th { text-align: left; font-weight: 600; font-size: 13px; color: #6b7280; border-bottom: 1px solid #e3e5ea; padding: 6px 8px; }
.holdings td { padding: 8px; border-bottom: 1px solid #f1f2f5; cursor: pointer; }
.holdings tr.held td { background: #eef4ff; }
.holdings .n { text-align: right; font-variant-numeric: tabular-nums; }
.holdings .sym { font-weight: 600; }
.holdings .up { color: #15803d; }
.holdings .down { color: #b91c1c; }
</style>
