<script>
  import { link } from "@snapfire/fsr-client/svelte";
  let { children, ...options } = $props();
</script>

<a {...link(options)}>{@render children?.()}</a>
