<script setup lang="ts">
import StatusDot from "./StatusDot.vue";
import EngineChip from "./EngineChip.vue";
import type { PluginRow } from "../types";

const props = defineProps<{ plugins: PluginRow[] }>();
const emit = defineEmits<{
  /** 行内主操作（安装/更新/重试，随状态变化） */
  primary: [id: string];
  uninstall: [id: string];
  openDir: [id: string];
  clearFilters: [];
}>();

const statusToDot: Record<PluginRow["status"], "ok" | "warn" | "idle" | "err"> = {
  installed: "ok",
  updatable: "warn",
  idle: "idle",
  error: "err",
};

const primaryLabel: Record<PluginRow["status"], string> = {
  installed: "重装",
  updatable: "更新",
  idle: "安装",
  error: "重试",
};

/** GitHub 来源 → 仓库地址（线上说明入口）；本地来源无线上页 */
function onlineUrl(p: PluginRow): string | null {
  if (p.source !== "github") return null;
  return `https://github.com/${p.origin.replace(/^.*github\.com\//, "")}`;
}
</script>

<template>
  <div class="cards" role="list">
    <div v-if="plugins.length === 0" class="empty">
      <div class="empty-title">没有匹配的插件</div>
      <button class="link-btn" @click="emit('clearFilters')">清除筛选</button>
    </div>

    <article
      v-for="p in plugins"
      :key="p.id"
      class="card"
      :class="{ stale: p.status === 'error' }"
      role="listitem"
    >
      <header class="head">
        <StatusDot :status="statusToDot[p.status]" />
        <span class="name">{{ p.name }}</span>
          <span class="host-badge" :class="p.host === 'UE' ? 'ue' : p.host === 'Houdini' ? 'hou' : 'obs'">{{ p.host }}</span>
      </header>

      <div v-if="p.version" class="ver mono">
        <template v-if="p.status === 'updatable'">
          <span class="old">v{{ p.version }}</span>
          <span class="arrow">→</span>
          <span class="new">v{{ p.latest }}</span>
        </template>
        <template v-else>v{{ p.version }}</template>
      </div>
      <div v-else class="ver mono dim">未安装</div>

      <p class="desc">{{ p.desc }}</p>

      <div v-if="p.engines.length" class="engines">
        <EngineChip v-for="e in p.engines" :key="e" :text="e" title="已安装引擎" />
      </div>

      <footer class="foot">
        <span class="origin" :title="p.origin">{{ p.origin }}</span>
        <a
          v-if="onlineUrl(p)"
          class="online"
          :href="onlineUrl(p)!"
          target="_blank"
          rel="noreferrer"
        >线上说明 ↗</a>
        <span class="spacer" />
        <button class="act icon" title="打开目录" @click="emit('openDir', p.id)">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round">
            <path d="M4 7a2 2 0 0 1 2-2h4l2 2h6a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2z" />
          </svg>
        </button>
        <button
          v-if="p.engines.length > 0"
          class="act"
          @click="emit('uninstall', p.id)"
        >卸载</button>
        <button class="act primary" @click="emit('primary', p.id)">{{ primaryLabel[p.status] }}</button>
      </footer>
    </article>
  </div>
</template>

<style scoped>
.cards {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(296px, 1fr));
  gap: 12px;
  align-content: start;
  padding: 14px 18px 26px;
}

.empty {
  grid-column: 1 / -1;
  height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
}
.empty-title {
  font-size: 13px;
  color: var(--text-2);
}
.link-btn {
  border: none;
  background: transparent;
  color: var(--c-primary);
  font-family: var(--font-stack);
  font-size: 12px;
  cursor: pointer;
}
.link-btn:hover {
  text-decoration: underline;
}

.card {
  display: flex;
  flex-direction: column;
  gap: 7px;
  padding: 14px 14px 11px;
  border-radius: var(--radius-pop);
  /* 隔层靠色块不靠线：无描边，填充分层 + hover 提亮 */
  background: rgba(255, 255, 255, 0.045);
  border: none;
  transition: background 0.15s, transform 0.15s;
}
.card:hover {
  background: rgba(255, 255, 255, 0.07);
  transform: translateY(-1px);
}
/* 失败态：左侧细红条示意，不再整圈红描边 */
.card.stale {
  box-shadow: inset 2.5px 0 0 rgba(255, 69, 58, 0.65);
}

.head {
  display: flex;
  align-items: center;
  gap: 9px;
  min-width: 0;
}
.name {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-1);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.host-badge {
  margin-left: auto;
  flex: none;
  height: 17px;
  display: inline-flex;
  align-items: center;
  padding: 0 6px;
  border-radius: 4px;
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.02em;
}
.host-badge.ue {
  background: rgba(10, 132, 255, 0.16);
  color: #6db2ff;
}
.host-badge.hou {
  background: rgba(255, 159, 10, 0.16);
  color: #ffb84d;
}
.host-badge.obs {
  background: rgba(167, 139, 250, 0.16);
  color: #c4b5fd;
}

.ver {
  font-size: 12px;
  color: var(--text-num);
}
.ver.dim {
  color: var(--text-2);
  font-family: var(--font-stack);
}
.ver .old {
  color: var(--text-2);
}
.ver .arrow {
  color: var(--text-sep);
  margin: 0 5px;
}
.ver .new {
  color: var(--c-warn);
}

.desc {
  font-size: 11.5px;
  line-height: 1.55;
  color: var(--text-2);
  min-height: 36px;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.engines {
  display: flex;
  gap: 4px;
  flex-wrap: wrap;
}

.foot {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 4px;
  /* 去横隔线：靠间距与字号/透明度分层，不切割卡片 */
}
.origin {
  font-family: var(--font-mono);
  font-size: 10px;
  color: var(--text-2);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 128px;
}
.online {
  flex: none;
  font-size: 11px;
  color: var(--c-primary);
  text-decoration: none;
  white-space: nowrap;
}
.online:hover {
  text-decoration: underline;
}
.spacer {
  flex: 1;
}

.act {
  flex: none;
  height: 26px;
  padding: 0 11px;
  border: none;
  border-radius: var(--radius-ctrl);
  background: rgba(255, 255, 255, 0.07);
  color: var(--text-2);
  font-family: var(--font-stack);
  font-size: 11.5px;
  cursor: pointer;
  transition: background 0.12s, color 0.12s, transform 0.12s;
}
.act:hover {
  background: var(--btn-hover);
  color: var(--text-1);
}
.act:active {
  transform: scale(0.94);
}
.act.primary {
  background: rgba(10, 132, 255, 0.18);
  color: #6db2ff;
}
.act.primary:hover {
  background: rgba(10, 132, 255, 0.28);
  color: #8cc2ff;
}

.act.icon {
  width: 26px;
  padding: 0;
  display: grid;
  place-items: center;
}
.act.icon svg {
  width: 14px;
  height: 14px;
}
</style>
