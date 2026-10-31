<script lang="ts" generics="Row extends { id: string }">
  import type { Snippet } from "svelte";

  let { rows, row, children, ...rest }: { rows: Row[]; row: Snippet<[Row, number]>; children?: Snippet; [key: string]: unknown } = $props();
</script>

<table class="ranks" {...rest}>
  {@render children?.()}
  <tbody>
    {#each rows as item, i (item.id)}
      {@render row(item, i)}
    {/each}
  </tbody>
</table>

<style>
  table { border-collapse: collapse; width: 100%; }
</style>
