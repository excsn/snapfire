<script lang="ts">
  import { recent } from "./visits.svelte.js";
  import { link } from "@snapfire/fsr-client/svelte";

  let { titles }: { titles: { id: string; title: string }[] } = $props();
  const shown = $derived(recent.ids.map((id) => titles.find((t) => t.id === id)).filter((t) => t !== undefined));
</script>

<section class="recent">
  <h2>Opened lately</h2>
  {#if shown.length === 0}
    <p class="none">Nothing yet.</p>
  {:else}
    <ol>
      {#each shown as book (book.id)}
        <li><a {...link({ href: `/book/${book.id}` })}>{book.title}</a></li>
      {/each}
    </ol>
  {/if}
</section>
