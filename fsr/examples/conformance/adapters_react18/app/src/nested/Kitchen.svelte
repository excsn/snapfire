<script lang="ts">
  import { setContext, type Snippet } from "svelte";

  let { children }: { children?: Snippet } = $props();
  const servings = $state({ count: 2 });
  setContext("servings", servings);
</script>

<div class="kitchen">
  <button class="kitchen-four" onclick={() => (servings.count = 4)}>Cook for four</button>
  {@render children?.()}
</div>
