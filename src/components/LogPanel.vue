<script setup lang="ts">
import { ref, watch, nextTick } from "vue";

const props = defineProps<{ lines: string[]; max?: number }>();

const box = ref<HTMLElement | null>(null);

watch(
  () => props.lines.length,
  async () => {
    await nextTick();
    box.value?.scrollTo({ top: box.value.scrollHeight });
  },
);
</script>

<template>
  <div ref="box" class="log mono">
    <div v-for="(l, i) in lines" :key="i" class="line" :class="{ err: l.startsWith('✗') }">
      {{ l }}
    </div>
    <div v-if="lines.length === 0" class="idle">等待输出…</div>
  </div>
</template>

<style scoped>
.log {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 10px 12px;
  border-radius: var(--radius-ctrl);
  background: var(--bg-sunken);
  font-size: 11px;
  line-height: 1.65;
  color: var(--text-2);
  white-space: pre-wrap;
  word-break: break-all;
}
.line.err {
  color: #ff8a80;
}
.idle {
  color: var(--text-sep);
}
</style>
