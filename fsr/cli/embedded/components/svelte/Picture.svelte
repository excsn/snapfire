<script>
  import { picture } from "@snapfire/fsr/client/svelte";
  let options = $props();
  let parts = $derived(picture(options));
</script>

{#if parts.sources}<picture>{#each parts.sources as source}<source {...source} />{/each}<img {...parts.img} /></picture>{:else}<img {...parts.img} />{/if}
