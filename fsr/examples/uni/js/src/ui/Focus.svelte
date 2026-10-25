<script lang="ts">
  import { key } from "@snapfire/fsr/client";
  import { island, useStore } from "@snapfire/fsr/client/svelte";

  interface Holding {
    symbol: string;
    name: string;
    shares: bigint;
    price: number;
    change: number;
  }

  let { holdings, watched }: { holdings: Holding[]; watched: string } = $props();
  // svelte-ignore state_referenced_locally
  const held = useStore(key<string>("uni/watched"), watched);
  const focus = $derived(holdings.find((row) => row.symbol === $held) ?? holdings[0]);
</script>

<aside class="focus">
  <div class="picks">
    {#each holdings as row (row.symbol)}
      <button type="button" class="pick" aria-pressed={row.symbol === $held} onclick={() => ($held = row.symbol)}>{row.symbol}</button>
    {/each}
  </div>
  {#if focus}
    <p class="focused"><strong class="symbol">{focus.symbol}</strong> {focus.name} at <span class="price">{focus.price.toFixed(2)}</span></p>
    <span class="move" {@attach island("js/src/ui/Chip.tsx#default", { symbol: focus.symbol, change: focus.change })}></span>
  {/if}
</aside>
