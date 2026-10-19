<script lang="ts">
  import { onDestroy, onMount, type Snippet } from "svelte";
  import { island, link, picture, useLocale, useStore } from "@snapfire/fsr-client/svelte";

  import { rendered, unmounted } from "@src/probes";
  import { probeCount } from "@src/store";

  let { label, nest = [], children }: { label: string; nest?: string[]; children?: Snippet } = $props();
  const count = useStore(probeCount, 0);
  const locale = useLocale();
  const image = picture({ src: "/pictures/probe.png", alt: "probe", width: 4, height: 4 });
  onMount(() => rendered("svelte"));
  onDestroy(() => unmounted("svelte"));
</script>

<section class="probe" data-owner="svelte">
  <h3 class="label">{label}</h3>
  <p class="count">{$count}</p>
  <button class="add" onclick={() => ($count = $count + 1)}>add</button>
  <p class="locale">{$locale}</p>
  <a {...link({ href: "/next" })} class="next">next</a>
  {#if image.sources}
    <picture>
      {#each image.sources as source, i (i)}<source {...source} />{/each}
      <img {...image.img} />
    </picture>
  {:else}
    <img {...image.img} />
  {/if}
  <div class="children">{@render children?.()}</div>
  {#each nest as module (module)}
    <div class="nest" {@attach island(module, { label: label + " nested", nest: [] })}></div>
  {/each}
</section>
