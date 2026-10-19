<script lang="ts">
  import type { Snippet } from "svelte";
  import { writable, derived } from "svelte/store";

  import Table from "./Table.svelte";
  import Legend from "./Legend.svelte";
  import Bars from "./Bars.svelte";
  import Dots from "./Dots.svelte";

  interface Row {
    id: string;
    title: string;
    author: string;
    pages: number;
    read: number;
    done: boolean;
  }

  let { rows, children }: { rows: Row[]; children?: Snippet } = $props();
  let order = $state("title");
  let dots = $state(false);
  const level = writable(2);
  const heading = derived(level, (n) => `h${n}`);
  const sorted = $derived([...rows].sort((a, b) => (order === "pages" ? b.pages - a.pages : a.title < b.title ? -1 : 1)));
  const View = $derived(dots ? Dots : Bars);
  const total = $derived(rows.reduce((sum, { read }) => sum + read, 0));
  const note = $derived(`<strong>${total}</strong> pages read in all`);
</script>

<section class="stats">
  <svelte:element this={$heading} class="title">Reading so far</svelte:element>
  {@render children?.()}
  <div class="controls">
    <label>Order <select bind:value={order}><option value="title">Title</option><option value="pages">Length</option></select></label>
    <label><input type="checkbox" bind:checked={dots} /> Dots</label>
  </div>
  <p class="note">{@html note}</p>
  <Table rows={sorted}>
    {#snippet row({ id, title, pages, read, done }, i)}
      {const share = Math.round((read / pages) * 100)}
      <tr class:done style:--share="{share}%">
        <td class="rank">{i + 1}</td>
        <th scope="row">{title}</th>
        <td class="share">
          {#key share}<View {share} />{/key}
        </td>
        <td class="count">{read} / {pages}</td>
      </tr>
    {/snippet}
    <caption>{rows.length} books</caption>
  </Table>
  <Legend {total} count={rows.length} class="legend" data-kind="pages">
    <span slot="title" let:average>About {average} pages a book</span>
    {#await total then pages}<em>{pages} pages</em>{/await}
  </Legend>
  <svelte:boundary>
    <p class="fine">Shares round to the nearest percent.</p>
    {#snippet failed()}<p>Could not count.</p>{/snippet}
  </svelte:boundary>
</section>

<style>
  .stats { display: grid; gap: 0.75rem; }
  .controls { display: flex; gap: 1rem; }
  tr.done th { text-decoration: line-through; }
  .note :global(strong) { font-weight: 700; }
  td, th { padding: 0.25rem 0.5rem; text-align: left; }
</style>
