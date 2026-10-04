<script setup lang="ts">
import { computed, ref } from "vue";

type Talk = { id: string; title: string; room: string; starts: string };

const props = defineProps<{ talks: Talk[] }>();
const rooms = computed(() => props.talks.map((t) => t.room).filter((room, i, all) => all.indexOf(room) === i));
const chosen = ref<string | null>(null);
const shown = computed(() => props.talks.filter((t) => t.room === chosen.value));
</script>

<template>
  <div class="floors">
    <div class="floors-rooms">
      <button v-for="room in rooms" :key="room" class="floors-room" :class="{ 'floors-chosen': room === chosen }" @click="chosen = room">{{ room }}</button>
    </div>
    <ol v-if="chosen" class="floors-talks">
      <li v-for="talk in shown" :key="talk.id">{{ talk.starts }} {{ talk.title }}</li>
    </ol>
  </div>
</template>
