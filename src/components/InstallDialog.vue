<script setup lang="ts">
import { ref, watch } from "vue";
import type { CompatStatusDto, EngineRow, PluginRow, PreflightItem } from "../api";

const props = defineProps<{
  visible: boolean;
  plugin: PluginRow | null;
  engines: EngineRow[];
  /** engine → 兼容状态（git 源才有；本地源为空 map = 全部可手选） */
  compat: Record<string, CompatStatusDto>;
  /** engine → 预检结果（M3：文件锁/引擎残缺等阻塞原因） */
  preflight: Record<string, PreflightItem[]>;
}>();
const emit = defineEmits<{ confirm: [engines: string[]]; cancel: [] }>();

const checked = ref<Set<string>>(new Set());

watch(
  () => props.visible,
  (v) => {
    if (!v) return;
    // 默认勾选：confirmed 可装的引擎；无 compat 信息（本地源）不预选
    const pre = new Set<string>();
    for (const [engine, s] of Object.entries(props.compat)) {
      if (s.kind === "installable" && s.confirmed) pre.add(engine);
    }
    checked.value = pre;
  }
);

/** 预检阻塞原因（不兼容/引擎残缺/文件锁），无则 null。 */
function blockedReason(engine: string): string | null {
  const items = props.preflight[engine];
  if (!items) return null;
  const bad = items.find((i) => i.blocking && !i.ok);
  return bad?.message ?? null;
}

function isBlocked(engine: string): boolean {
  return statusOf(engine)?.kind === "incompatible" || blockedReason(engine) !== null;
}function rowValue(e: { id?: string; version: string }): string {
  return e.id ?? e.version;
}

function toggle(row: { id?: string; version: string }, disabled: boolean) {
  if (disabled) return;
  const v = rowValue(row);
  const next = new Set(checked.value);
  if (next.has(v)) next.delete(v);
  else next.add(v);
  checked.value = next;
}

function statusOf(engine: string): CompatStatusDto | null {
  return props.compat[engine] ?? null;
}

interface Badge {
  cls: string;
  text: string;
  title: string;
}
function badgeOf(s: CompatStatusDto | null): Badge | null {
  if (!s) return null;
  switch (s.kind) {
    case "installed":
      return { cls: "ok", text: "已装", title: "该引擎已有安装记录" };
    case "installable":
      return s.confirmed
        ? { cls: "ok", text: `✓ ${s.gitRef}`, title: "分支 EngineVersion 已确认匹配" }
        : { cls: "warn", text: s.gitRef, title: "版本匹配未确认（家族分支/弱提示）" };
    case "unverified":
      return { cls: "warn", text: "未验证·试编译", title: "无版本信号——安装即试编译，成败为最终裁决" };
    case "incompatible":
      return { cls: "bad", text: "不兼容", title: s.reason };
  }
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
        <div class="sub">
          {{
            plugin.host === "Obsidian"
              ? "安装到 vault 的 .obsidian\\plugins\\（完成后在 Obsidian 设置中启用）"
              : plugin.host === "Houdini"
                ? "拷贝到偏好目录 plugins\\ + 生成 packages json（Houdini 重启后生效）"
                : plugin.source === "github"
                  ? "GitHub 源：Release 附件优先，无附件走 RunUAT 源码构建"
                  : "本地目录拷贝到 Engine\\Plugins\\Marketplace"
          }}
        </div>

        <div class="engine-list">
          <label
            v-for="e in engines"
            :key="rowValue(e)"
            class="engine-item"
            :class="{
              checked: checked.has(rowValue(e)),
              banned: isBlocked(rowValue(e)),
            }"
            :title="blockedReason(rowValue(e)) ?? badgeOf(statusOf(rowValue(e)))?.title ?? ''"
          >
            <input
              type="checkbox"
              :checked="checked.has(rowValue(e))"
              :disabled="isBlocked(rowValue(e))"
              @change="toggle(e, isBlocked(rowValue(e)))"
            />
            <span class="ver mono">{{ e.version }}</span>
            <span v-if="badgeOf(statusOf(rowValue(e)))" class="badge" :class="badgeOf(statusOf(rowValue(e)))!.cls">
              {{ badgeOf(statusOf(rowValue(e)))!.text }}
            </span>
            <span class="path">{{ e.root }}</span>
          </label>
        </div>

        <div class="hint">✓ 绿=分支已确认 · 黄=未验证可试编译 · 红=不兼容或预检未过（悬停看原因）</div>

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
.engine-item.banned {
  opacity: 0.55;
  cursor: not-allowed;
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

.badge {
  flex: none;
  height: 17px;
  display: inline-flex;
  align-items: center;
  padding: 0 7px;
  border-radius: 9px;
  font-size: 10px;
  font-weight: 500;
  max-width: 150px;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}
.badge.ok {
  background: rgba(48, 209, 88, 0.13);
  color: var(--c-ok);
}
.badge.warn {
  background: rgba(255, 214, 10, 0.12);
  color: var(--c-warn);
}
.badge.bad {
  background: rgba(255, 69, 58, 0.13);
  color: var(--c-err);
}

.path {
  font-size: 10.5px;
  color: var(--text-2);
  font-family: var(--font-mono);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  flex: 1;
  text-align: right;
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
