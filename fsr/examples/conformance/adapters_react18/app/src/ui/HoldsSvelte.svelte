<script lang="ts">
  import { Place } from "@snapfire/fsr/components";

  let n = $state(0);
</script>

<section class="holds" data-holder="svelte">
  <button class="again" onclick={() => (n = n + 1)}>{n}</button>
  <Place name="svelte-place" />
</section>
