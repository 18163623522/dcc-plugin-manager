<script setup lang="ts">
import { ref, watch } from "vue";
import type { PluginRow, UeEngine } from "../api";

const props = defineProps<{
  visible: boolean;
  plugin: PluginRow | null;
  engines: UeEngine[];
}>();
const emit = defineEmits<{ confirm: [engines: string[]]; cancel: [] }>();

const checked = ref<Set<string>>(new Set());

watch(
  () => props.visible,
  (v) => {
    if (v) checked.value = new Set();
  }
);

function toggle(v: string) {
  const next = new Set(checked.value);
  if (next.has(v)) next.delete(v);
  else next.add(v);
  checked.value = next;
}

function confirm() {
  emit("confirm", [...checked.value]);
}
</script>

<template>
  <Teleport to="body">
    <div v-if="visible && plugin" class="overlay" @click.self="emit('cancel')">
      <div class="dialog">
        <div class="title">安装 {{ plugin.name }}</div>
        <div class="sub">选择目标引擎（拷贝到 Engine\Plugins\Marketplace）</div>

        <div class="engine-list">
          <label
            v-for="e in engines"
            :key="e.version"
            class="engine-item"
            :class="{ checked: checked.has(e.version) }"
          >
            <input
              type="checkbox"
              :checked="checked.has(e.version)"
              @change="toggle(e.version)"
            />
            <span class="ver mono">{{ e.version }}</span>
            <span class="path">{{ e.root }}</span>
          </label>
        </div>

        <div class="hint">兼容矩阵（按分支/EngineVersion 自动勾选）在里程碑 2 提供，请自行确认版本匹配</div>

        <div class="footer">
          <button class="btn" @click="emit('cancel')">取消</button>
          <button class="btn primary" :disabled="checked.size === 0" @click="confirm">
            安装到 {{ checked.size }} 个引擎
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
  width: 520px;
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

.engine-list {
  margin-top: 14px;
  display: flex;
  flex-direction: column;
  gap: 4px;
  overflow-y: auto;
  min-height: 60px;
}

.engine-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 10px;
  border-radius: var(--radius-ctrl);
  cursor: pointer;
  border: 1px solid transparent;
  transition: background 0.12s;
}
.engine-item:hover {
  background: rgba(255, 255, 255, 0.05);
}
.engine-item.checked {
  background: rgba(10, 132, 255, 0.1);
  border-color: rgba(10, 132, 255, 0.35);
}
.engine-item input {
  accent-color: var(--c-primary);
}
.ver {
  font-size: 13px;
  color: var(--text-num);
  font-weight: 600;
  flex: none;
}
.path {
  font-size: 10.5px;
  color: var(--text-2);
  font-family: var(--font-mono);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.hint {
  margin-top: 12px;
  font-size: 10.5px;
  color: var(--text-sep);
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
.btn.primary {
  background: var(--c-primary);
  color: #fff;
}
.btn.primary:hover {
  filter: brightness(1.12);
}
.btn.primary:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
</style>
