<script lang="ts">
  import { onMount } from "svelte";
  import { get, optimistic } from "@snapfire/fsr-client";

  import { actions } from "@generated/client";
  import { doneCount } from "@src/store";
  import { visit } from "./visits.svelte.js";

  let { id, pages, read, done, current }: { id: string; pages: number; read: number; done: boolean; current: boolean } = $props();
  // svelte-ignore state_referenced_locally
  let page = $state(read);
  // svelte-ignore state_referenced_locally
  let finished = $state(done);
  // svelte-ignore state_referenced_locally
  let reading = $state(current);
  let why = $state("");
  const percent = $derived(Math.round((page / pages) * 100));

  onMount(() => visit(id));

  async function save(next: number): Promise<void> {
    const ends = Math.min(Math.max(next, 0), pages) === pages;
    const step = ends === finished ? 0 : ends ? 1 : -1;
    try {
      const answer = await optimistic(doneCount, (get(doneCount) ?? 0) + step, () => actions.book.$id.progress({ book_id: id, page: next }));
      page = answer.page;
      finished = answer.finished;
      why = "";
    } catch (e) {
      why = e instanceof Error ? e.message : "that did not take";
    }
  }

  async function pick(): Promise<void> {
    await actions.book.$id.pick({ book_id: id });
    reading = true;
  }
</script>

<section class="progress">
  <p class="count"><span class="page">{page}</span> of {pages} pages, <span class="percent">{percent}%</span></p>
  <div class="bar"><span class="fill" style:width="{percent}%"></span></div>
  <p class="actions">
    <button type="button" class="more" onclick={() => save(page + 10)} disabled={finished}>Read 10 more</button>
    <button type="button" class="finish" onclick={() => save(pages)} disabled={finished}>Finish</button>
    <button type="button" class="restart" onclick={() => save(0)}>Start again</button>
    {#if !reading}
      <button type="button" class="pick" onclick={pick}>Reading this now</button>
    {/if}
  </p>
  {#if finished}<p class="finished">Finished.</p>{/if}
  {#if why !== ""}<p class="why">{why}</p>{/if}
</section>
