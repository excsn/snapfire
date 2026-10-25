<script module>
  import Pieces from "./Pieces.svelte";
  export { tree };
</script>

<script>
  import Island from "./Island.svelte";
  import { held, opaque } from "@snapfire/fsr/client/svelte";
  let { pieces } = $props();
  const list = held(() => pieces);
  const own = opaque();
</script>

{#snippet tree()}<sf-s data-sf-children=""><Pieces /></sf-s>{/snippet}

{#each list() as piece}{#if piece.kind === "island"}<Island {piece} />{:else if piece.kind === "element"}<svelte:element this={piece.tag} {...piece.attrs}><Pieces pieces={piece.children} /></svelte:element>{:else if piece.kind === "opaque"}<svelte:element this={piece.tag} {...piece.attrs} {@attach own(piece)} />{:else}{@html piece.html}{/if}{/each}
