<script setup>
import { computed } from 'vue'
import { valueToPoint } from './util.js'

const props = defineProps({
  stat: Object,
  index: Number,
  total: Number
})

const point = computed(() =>
  valueToPoint(+props.stat.value + 10, props.index, props.total)
)
</script>

<template>
  <text :x="point.x" :y="point.y">{{stat.label}}</text>
</template>