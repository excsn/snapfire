<script lang="ts">
  import { getContext } from "svelte";

  const servings = getContext<{ count: number } | undefined>("servings");
  const shown = Boolean(window);
</script>

<p class="timer">{shown ? "Timing" : "Waiting"} for {servings?.count ?? 1}</p>
