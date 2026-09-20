<script setup lang="ts">
defineProps<{
  text: string;
  title?: string;
  /** 兼容徽标语义色（缺省中性） */
  tone?: "ok" | "warn" | "bad";
}>();
</script>

<template>
  <span class="chip mono" :class="tone" :title="title ?? text">{{ text }}</span>
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
.chip.ok {
  background: rgba(48, 209, 88, 0.12);
  border-color: rgba(48, 209, 88, 0.25);
  color: var(--c-ok);
}
.chip.warn {
  background: rgba(255, 214, 10, 0.1);
  border-color: rgba(255, 214, 10, 0.22);
  color: var(--c-warn);
}
.chip.bad {
  background: rgba(255, 69, 58, 0.12);
  border-color: rgba(255, 69, 58, 0.25);
  color: var(--c-err);
}
</style>
