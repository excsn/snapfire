<!--
https://eugenkiss.github.io/7guis/tasks/#counter
-->

<script setup>
import { ref } from 'vue'

const count = ref(0)
</script>

<template>
  {{ count }}
  <button @click="count++">Count</button></template>