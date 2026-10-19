<script>
import { valueToPoint } from './util.js'

export default {
  props: {
    stat: Object,
    index: Number,
    total: Number
  },
  computed: {
    point: function () {
      return valueToPoint(+this.stat.value + 10, this.index, this.total)
    }
  }
}
</script>

<template>
  <text :x="point.x" :y="point.y">{{stat.label}}</text>
</template>