<script setup lang="ts">
import LogPanel from "./LogPanel.vue";

defineProps<{
  visible: boolean;
  title: string;
  lines: string[];
  cancellable: boolean;
  done: boolean;
}>();
const emit = defineEmits<{ cancel: []; close: [] }>();
</script>

<template>
  <Teleport to="body">
    <div v-if="visible" class="overlay">
      <div class="dialog">
        <div class="head">
          <span class="title">
            {{ title }}
            <span v-if="done" class="state ok">完成</span>
            <span v-else class="state busy breathe">进行中</span>
          </span>
          <div class="ops">
            <button v-if="cancellable && !done" class="btn danger" @click="emit('cancel')">取消构建</button>
            <button v-if="done" class="btn" @click="emit('close')">关闭</button>
          </div>
        </div>
        <LogPanel :lines="lines" />
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  z-index: 110;
  background: rgba(0, 0, 0, 0.4);
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
  width: 760px;
  height: 480px;
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
  padding: 18px;
  animation: pop-in 0.18s var(--ease-panel);
}
@keyframes pop-in {
  from {
    opacity: 0;
    transform: translateY(8px) scale(0.98);
  }
}

.head {
  flex: none;
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 12px;
}
.title {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-1);
}
.state {
  margin-left: 10px;
  font-size: 11px;
  font-weight: 500;
  padding: 2px 9px;
  border-radius: 9px;
}
.state.ok {
  background: rgba(48, 209, 88, 0.13);
  color: var(--c-ok);
}
.state.busy {
  background: rgba(255, 159, 10, 0.13);
  color: var(--c-busy);
}

.ops {
  display: flex;
  gap: 8px;
}
.btn {
  height: 28px;
  padding: 0 13px;
  border: none;
  border-radius: var(--radius-ctrl);
  background: rgba(255, 255, 255, 0.07);
  color: var(--text-1);
  font-family: var(--font-stack);
  font-size: 12px;
  cursor: pointer;
}
.btn:hover {
  background: var(--btn-hover);
}
.btn.danger {
  background: rgba(255, 69, 58, 0.85);
  color: #fff;
}
.btn.danger:hover {
  filter: brightness(1.12);
}
</style>
