<script lang="ts">
  import type { Snippet } from "svelte";

  let { children }: { children?: Snippet } = $props();
  let n = $state(0);
</script>

<section class="frame" data-frame="svelte">
  <button class="tick" onclick={() => (n = n + 1)}>{n}</button>
  {@render children?.()}
</section>
