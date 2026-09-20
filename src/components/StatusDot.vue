<script setup lang="ts">
import { computed } from "vue";

const props = defineProps<{ status: "ok" | "warn" | "idle" | "err" | "busy" }>();

const labels: Record<string, string> = {
  ok: "已装最新",
  warn: "可更新",
  idle: "未安装",
  err: "安装失败",
  busy: "进行中",
};
const label = computed(() => labels[props.status] ?? props.status);
</script>

<template>
  <span class="dot" :class="[status, { breathe: status === 'err' }]" :title="label" />
</template>

<style scoped>
.dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex: none;
}
.ok {
  background: var(--c-ok);
}
.warn {
  background: var(--c-warn);
}
.idle {
  background: var(--c-idle);
}
.err {
  background: var(--c-err);
}
.busy {
  background: var(--c-busy);
}
</style>
