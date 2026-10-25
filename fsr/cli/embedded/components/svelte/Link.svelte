<script>
  import { holdLink, link } from "@snapfire/fsr/client/svelte";
  let { children, ...options } = $props();
  let anchor = $state(null);
  holdLink(() => anchor);
</script>

<a bind:this={anchor} {...link(options)}>{@render children?.()}</a>
