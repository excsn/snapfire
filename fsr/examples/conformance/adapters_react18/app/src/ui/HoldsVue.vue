<script setup lang="ts">
import { ref } from "vue";
import { Place } from "@snapfire/fsr/components";

const n = ref(0);
</script>

<template>
  <section class="holds" data-holder="vue">
    <button class="again" @click="n = n + 1">{{ n }}</button>
    <Place name="vue-place" />
  </section>
</template>
