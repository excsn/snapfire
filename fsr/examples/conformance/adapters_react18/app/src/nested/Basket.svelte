<script lang="ts">
  let asked = $state(false);

  function label(): string {
    if (asked) throw new Error("the basket is unavailable");
    return "Fill the basket";
  }
</script>

<button class="basket" onclick={() => (asked = true)}>{label()}</button>
