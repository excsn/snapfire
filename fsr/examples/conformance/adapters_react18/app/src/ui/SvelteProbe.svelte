<script lang="ts">
  import { onDestroy, onMount, type Snippet } from "svelte";
  import { useLocale, useStore } from "@snapfire/fsr/client/svelte";
  import { Link, Mount, Picture } from "@snapfire/fsr/components";

  import { rendered, unmounted } from "@src/probes";
  import { probeCount } from "@src/store";

  let { label, nest = [], children }: { label: string; nest?: string[]; children?: Snippet } = $props();
  const count = useStore(probeCount, 0);
  const locale = useLocale();
  onMount(() => rendered("svelte"));
  onDestroy(() => unmounted("svelte"));
</script>

<section class="probe" data-owner="svelte">
  <h3 class="label">{label}</h3>
  <p class="count">{$count}</p>
  <button class="add" onclick={() => ($count = $count + 1)}>add</button>
  <p class="locale">{$locale}</p>
  <Link href="/next" class="next">next</Link>
  <Picture src="/pictures/probe.png" alt="probe" width={4} height={4} />
  <div class="children">{@render children?.()}</div>
  {#each nest as module (module)}
    <Mount {module} props={{ label: label + " nested", nest: [] }} />
  {/each}
</section>
