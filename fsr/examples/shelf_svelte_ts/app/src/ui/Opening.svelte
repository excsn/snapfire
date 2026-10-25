<script lang="ts">
  import { useLinkStatus } from "@snapfire/fsr/client/svelte";

  const status = useLinkStatus();
</script>

<span class={$status.pending ? "opening opening-on" : "opening"}>{$status.pending ? "opening…" : ""}</span>
