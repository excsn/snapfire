<script>
  import { mount } from "@snapfire/fsr/client/svelte";
  let { module, props = {}, when } = $props();
</script>

<sf-s data-sf-island="" data-sf-when={when} {@attach mount(module, props, when)}></sf-s>
