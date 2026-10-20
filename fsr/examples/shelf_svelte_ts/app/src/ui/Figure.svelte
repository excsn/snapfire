<script module>
  let figures = 0;

  function number() {
    figures += 1;
    return figures;
  }
</script>

<script>
  let { caption } = $props();
  const n = number();
</script>

<p class="figure">Figure {n}: {caption}</p>
