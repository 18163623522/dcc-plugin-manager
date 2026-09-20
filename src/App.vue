<script setup lang="ts">
import { ref, shallowRef, type Component } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { inTauri } from "./api";
import PluginsView from "./views/Plugins.vue";
import EnginesView from "./views/Engines.vue";
import SettingsView from "./views/Settings.vue";

const win = inTauri ? getCurrentWindow() : null;
if (!inTauri) {
  // 浏览器预览模式：无 Tauri 壳，铺背景展示毛玻璃效果
  document.body.classList.add("browser-preview");
}

function minimize() {
  win?.minimize();
}
function closeApp() {
  win?.close();
}
function toggleMax() {
  win?.toggleMaximize();
}

interface NavItem {
  id: string;
  label: string;
  comp: Component;
  icon: string;
}

const views: NavItem[] = [
  { id: "plugins", label: "插件", comp: PluginsView, icon: "M3 3h8v8H3zM13 3h8v8h-8zM3 13h8v8H3zM13 13h8v8h-8z" },
  { id: "engines", label: "引擎", comp: EnginesView, icon: "M9 3v4M15 3v4M9 17v4M15 17v4M3 9h4M3 15h4M17 9h4M17 15h4M7 7h10v10H7z" },
  { id: "settings", label: "设置", comp: SettingsView, icon: "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33h.01a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51h.01a1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82v.01a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" },
];

const current = ref("plugins");
const currentView = shallowRef<Component>(PluginsView);

function switchTo(id: string) {
  const item = views.find((v) => v.id === id);
  if (item) {
    current.value = item.id;
    currentView.value = item.comp;
  }
}
</script>

<template>
  <div id="shell">
    <aside id="sidebar">
      <div class="brand">
        <div class="brand-name">DCC 插件管理</div>
        <div class="brand-sub">UE · Houdini</div>
      </div>
      <nav class="nav">
        <button
          v-for="v in views"
          :key="v.id"
          class="nav-item"
          :class="{ active: current === v.id }"
          @click="switchTo(v.id)"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
            <path :d="v.icon" />
          </svg>
          <span>{{ v.label }}</span>
        </button>
      </nav>
      <div class="side-foot">
        <span class="mono num">v0.1.0</span>
        <span class="sep">·</span>
        <span>M1</span>
      </div>
    </aside>

    <div id="main">
      <header id="titlebar" data-tauri-drag-region @dblclick="toggleMax">
        <div class="title" data-tauri-drag-region>{{ views.find((v) => v.id === current)?.label }}</div>
        <div class="win-ctrls">
          <button class="wbtn" title="最小化" @click="minimize">
            <svg viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><path d="M5 12h14" /></svg>
          </button>
          <button class="wbtn" title="最大化" @click="toggleMax">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"><rect x="6" y="6" width="12" height="12" rx="1.5" /></svg>
          </button>
          <button class="wbtn close" title="关闭" @click="closeApp">
            <svg viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18" /></svg>
          </button>
        </div>
      </header>
      <main id="content">
        <component :is="currentView" />
      </main>
    </div>
  </div>
</template>

<style scoped>
#shell {
  position: absolute;
  inset: 0;
  display: flex;
  background-color: var(--shell-bg);
  /* 噪点颗粒（overlay 混入底色）+ 顶部受光渐变：亚克力质感双层 */
  background-image:
    linear-gradient(180deg, rgba(255, 255, 255, 0.035), rgba(255, 255, 255, 0) 150px),
    var(--noise);
  background-blend-mode: normal, overlay;
  backdrop-filter: blur(var(--shell-blur)) saturate(var(--shell-saturate));
  border: var(--shell-border);
  border-radius: var(--shell-radius);
  box-shadow:
    var(--shell-shadow),
    inset 0 1px 0 rgba(255, 255, 255, 0.06); /* 玻璃上缘受光 */
  overflow: hidden;
}

/* —— 侧栏 —— */
#sidebar {
  width: 196px;
  flex: none;
  display: flex;
  flex-direction: column;
  background: var(--bg-panel);
  border-right: 1px solid rgba(255, 255, 255, 0.05);
  padding: 14px 10px;
}

.brand {
  padding: 4px 10px 14px;
}
.brand-name {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-1);
}
.brand-sub {
  font-size: 11px;
  color: var(--text-2);
  margin-top: 2px;
}

.nav {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.nav-item {
  position: relative;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 10px;
  border: none;
  border-radius: var(--radius-ctrl);
  background: transparent;
  color: var(--text-2);
  font-family: var(--font-stack);
  font-size: 13px;
  cursor: pointer;
  transition: background 0.12s, color 0.12s;
}
.nav-item svg {
  width: 16px;
  height: 16px;
  flex: none;
}
.nav-item:hover {
  background: rgba(255, 255, 255, 0.06);
  color: var(--text-1);
}
.nav-item.active {
  background: rgba(255, 255, 255, 0.08);
  color: var(--text-1);
}
.nav-item.active::before {
  content: "";
  position: absolute;
  left: 0;
  top: 50%;
  transform: translateY(-50%);
  width: 3px;
  height: 16px;
  border-radius: 2px;
  background: var(--c-primary);
}

.side-foot {
  margin-top: auto;
  padding: 8px 10px 2px;
  font-size: 11px;
  color: var(--text-2);
}
.side-foot .sep {
  color: var(--text-sep);
  margin: 0 4px;
}

/* —— 主列 —— */
#main {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-width: 0;
}

#titlebar {
  height: 44px;
  flex: none;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding-left: 18px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.05);
}
.title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-1);
}

.win-ctrls {
  display: flex;
  height: 100%;
}

.wbtn {
  width: 46px;
  height: 100%;
  display: grid;
  place-items: center;
  border: none;
  background: transparent;
  color: var(--text-2);
  cursor: pointer;
  transition: background 0.12s, color 0.12s;
}
.wbtn svg {
  width: 16px;
  height: 16px;
}
.wbtn:hover {
  background: var(--btn-hover);
  color: var(--text-1);
}
.wbtn:active {
  background: var(--btn-active);
}
.wbtn.close:hover {
  background: #e81123;
  color: #fff;
}

#content {
  flex: 1;
  min-height: 0;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}
</style>
