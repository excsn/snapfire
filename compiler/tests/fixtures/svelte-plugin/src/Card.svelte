<script lang="ts">
  import { shout } from "./util";
  let { title }: { title: string } = $props();
  let n = $state(0);
</script>

<div class="card">
  <h2>{shout(title)}</h2>
  <button onclick={() => n++}>clicked {n}</button>
</div>

<style>
  .card { padding: 12px; }
</style>
