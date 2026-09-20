<script setup lang="ts">
import type { TargetPath } from "../api";

defineProps<{
  visible: boolean;
  title: string;
  paths: TargetPath[];
  busy?: boolean;
}>();
const emit = defineEmits<{ confirm: []; cancel: [] }>();
</script>

<template>
  <Teleport to="body">
    <div v-if="visible" class="overlay" @click.self="!busy && emit('cancel')">
      <div class="dialog">
        <div class="title">{{ title }}</div>
        <div class="sub">以下路径将被删除（只删注册表记录过的精确位置）：</div>

        <div class="path-list">
          <div v-for="(t, i) in paths" :key="i" class="path-item">
            <span class="engine mono">{{ t.engine }}</span>
            <span class="p">{{ t.path }}</span>
          </div>
        </div>

        <div class="footer">
          <button class="btn" :disabled="busy" @click="emit('cancel')">取消</button>
          <button class="btn danger" :disabled="busy" @click="emit('confirm')">
            {{ busy ? "执行中…" : "确认卸载" }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  z-index: 100;
  background: rgba(0, 0, 0, 0.32);
  display: grid;
  place-items: center;
  animation: fade 0.15s ease-out;
}
@keyframes fade {
  from {
    opacity: 0;
  }
}

.dialog {
  width: 560px;
  max-height: 80vh;
  display: flex;
  flex-direction: column;
  background-color: var(--bg-pop);
  background-image: var(--noise);
  background-blend-mode: overlay;
  backdrop-filter: blur(20px) saturate(var(--shell-saturate));
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: var(--radius-pop);
  box-shadow:
    0 12px 40px rgba(0, 0, 0, 0.5),
    inset 0 1px 0 rgba(255, 255, 255, 0.06);
  padding: 20px;
  animation: pop-in 0.18s var(--ease-panel);
}
@keyframes pop-in {
  from {
    opacity: 0;
    transform: translateY(8px) scale(0.98);
  }
}

.title {
  font-size: 15px;
  font-weight: 600;
  color: var(--text-1);
}
.sub {
  font-size: 11.5px;
  color: var(--text-2);
  margin-top: 3px;
}

.path-list {
  margin-top: 14px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  overflow-y: auto;
  max-height: 320px;
}

.path-item {
  display: flex;
  align-items: baseline;
  gap: 10px;
  padding: 7px 10px;
  border-radius: var(--radius-ctrl);
  background: var(--bg-sunken);
}
.engine {
  flex: none;
  font-size: 11px;
  color: var(--text-num);
  font-weight: 600;
}
.p {
  font-size: 10.5px;
  color: var(--text-2);
  font-family: var(--font-mono);
  word-break: break-all;
}

.footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 16px;
}

.btn {
  height: 30px;
  padding: 0 14px;
  border: none;
  border-radius: var(--radius-ctrl);
  background: rgba(255, 255, 255, 0.07);
  color: var(--text-1);
  font-family: var(--font-stack);
  font-size: 12px;
  cursor: pointer;
  transition: background 0.12s;
}
.btn:hover {
  background: var(--btn-hover);
}
.btn.danger {
  background: var(--c-err);
  color: #fff;
}
.btn.danger:hover {
  filter: brightness(1.12);
}
.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
