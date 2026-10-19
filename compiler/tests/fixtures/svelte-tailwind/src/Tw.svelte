<script lang="ts">
  let { on, tone }: { on: boolean; tone: string } = $props();
</script>

<div class="p-4 text-lg">
  <p class={on ? "font-bold" : "font-light"}>a</p>
  <p class={["rounded", { "ring-2": on }]}>b</p>
  <p class="tracking-wide" class:text-red-500={on} class:underline={!on}>c</p>
  <p class={`m-2 ${tone}`}>d</p>
</div>
