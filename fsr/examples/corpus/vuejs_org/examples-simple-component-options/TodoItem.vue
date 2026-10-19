<script>
export default {
  props: {
    todo: Object
  }
}
</script>

<template>
  <li>{{ todo.text }}</li></template>