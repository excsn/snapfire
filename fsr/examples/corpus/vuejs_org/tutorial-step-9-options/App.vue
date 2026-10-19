<script>
export default {
  mounted() {
    this.$refs.pElementRef.textContent = 'mounted!'
  }
}
</script>

<template>
  <p ref="pElementRef">Hello</p>
</template>