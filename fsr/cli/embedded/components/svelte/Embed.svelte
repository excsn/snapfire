<script>
  import { embed } from "@snapfire/fsr/client/svelte";
  let options = $props();
  let parts = $derived(embed(options));
</script>

<div {...parts.outer}><button {...parts.button}>{#if parts.still}<img {...parts.still} />{/if}<span {...parts.title}>{options.title}</span><span {...parts.play}>▶</span></button></div>
