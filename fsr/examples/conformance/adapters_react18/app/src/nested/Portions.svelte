<script lang="ts">
  import { getContext } from "svelte";

  const servings = getContext<{ count: number } | undefined>("servings");
</script>

<p class="portions">Serves {servings?.count ?? 1}</p>
