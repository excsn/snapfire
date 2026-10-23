<script lang="ts">
  import type { Snippet } from "svelte";

  let { children }: { children?: Snippet } = $props();
</script>

<svelte:boundary>
  <div class="kitchen-body">{@render children?.()}</div>
  {#snippet failed(_error, reset)}
    <div class="kitchen-failed">
      The kitchen hit a problem.
      <button class="kitchen-retry" onclick={reset}>Try again</button>
    </div>
  {/snippet}
</svelte:boundary>
