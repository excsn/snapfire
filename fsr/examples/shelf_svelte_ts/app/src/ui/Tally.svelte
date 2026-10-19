<script>
  import { useStore } from "@snapfire/fsr-client/svelte";
  import { doneCount } from "@src/store";

  export let done;
  export let total;
  const count = useStore(doneCount, done);
  $: left = total - $count;
</script>

<p class="tally"><strong class="done">{$count}</strong> finished, <span class="left">{left}</span> to go</p>
