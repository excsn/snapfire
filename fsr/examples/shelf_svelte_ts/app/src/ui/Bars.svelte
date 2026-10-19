<script lang="ts">
  let { share }: { share: number } = $props();
</script>

<span class="bar"><span class="fill" style:width="{share}%"></span></span>

<style>
  .bar { display: inline-block; width: 8rem; height: 0.5rem; background: #ddd; }
  .fill { display: block; height: 100%; background: #4a7; }
</style>
