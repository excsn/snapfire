<script>
  export let total = 0;
  export let count = 1;
  let average = 0;
  let shown;
  $: average = count > 0 ? Math.round(total / count) : 0;
  $: if (total > 0) shown = `${total} pages`;
</script>

<div {...$$restProps}>
  {#if $$slots.title}
    <h3><slot name="title" {average} /></h3>
  {/if}
  <p><slot>{shown ?? "Nothing read yet"}</slot></p>
</div>
