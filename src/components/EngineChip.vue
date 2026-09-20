<script setup lang="ts">
defineProps<{ text: string; title?: string }>();
</script>

<template>
  <span class="chip mono" :title="title ?? text">{{ text }}</span>
</template>

<style scoped>
.chip {
  display: inline-flex;
  align-items: center;
  height: 18px;
  padding: 0 7px;
  border-radius: 9px;
  background: rgba(255, 255, 255, 0.07);
  border: 1px solid rgba(255, 255, 255, 0.06);
  color: var(--text-num);
  font-size: 10.5px;
  font-weight: 500;
  white-space: nowrap;
}
</style>
