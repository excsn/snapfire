<script>
  let { name, as = "div", ...attributes } = $props();
</script>

<svelte:element this={as} {...attributes} id={name}></svelte:element>
