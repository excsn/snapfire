<script lang="ts">
  let { share }: { share: number } = $props();
  const filled = $derived(Math.round(share / 20));
</script>

<span class="dots">{#each [0, 1, 2, 3, 4] as n}<i class:on={n < filled}>●</i>{/each}</span>

<style>
  i { color: #ccc; font-style: normal; }
  .on { color: #4a7; }
</style>
