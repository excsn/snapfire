<script>
  import { tree } from "./Pieces.svelte";
  import { nested } from "@snapfire/fsr-client/svelte";
  let { piece } = $props();
  const island = nested(() => piece);
</script>

<sf-s data-sf-island="" data-sf-region={piece.region} data-sf-when={piece.when} data-sf-mode={piece.mode}><sf-i id={piece.id} data-sf-module={piece.moduleId} data-sf-ssr={piece.ssr ? "" : undefined} {@attach island.attach}>{#if island.component}<island.component {...island.props} children={piece.children ? tree : undefined} />{:else if piece.children}<template data-sf-children=""></template>{/if}</sf-i><script type="application/json" data-sf-props={piece.id}></script></sf-s>
