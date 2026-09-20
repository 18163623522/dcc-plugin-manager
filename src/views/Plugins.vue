<script setup lang="ts">
import { computed, reactive } from "vue";
import FilterBar from "../components/FilterBar.vue";
import PluginList from "../components/PluginList.vue";
import { DEFAULT_FILTER, type FilterState, type PluginRow } from "../types";

/* —— 占位数据（布局评审用；Task 1.8 换成 commands 返回的真实 registry 数据）—— */
const plugins: PluginRow[] = [
  {
    id: "TrueGlow",
    name: "TrueGlow",
    origin: "18163623522/TrueGlow",
    host: "UE",
    version: "0.8.2",
    latest: "0.9.0",
    engines: ["4.26", "5.7", "5.8"],
    status: "updatable",
    source: "github",
    desc: "物理辉光：HDR 金字塔 Bloom + Streak + 星芒",
  },
  {
    id: "BetterHLSL",
    name: "BetterHLSL",
    origin: "18163623522/BetterHLSL",
    host: "UE",
    version: "2.1.0",
    latest: "2.1.0",
    engines: ["4.26", "5.7"],
    status: "installed",
    source: "github",
    desc: "HLSL 补全对标 Visual Studio",
  },
  {
    id: "HoudiniGraphTools",
    name: "HoudiniGraphTools",
    origin: "18163623522/houdini-graph-tools",
    host: "UE",
    version: "2.0.0",
    latest: "2.0.0",
    engines: ["4.26"],
    status: "installed",
    source: "github",
    desc: "摇晃断线修复 + L 键自动排布",
  },
  {
    id: "MultiPointSDF",
    name: "多点 SDF",
    origin: "D:\\...\\MultiPointSDF\\otls",
    host: "Houdini",
    version: "1.1.0",
    latest: "1.1.0",
    engines: ["20.0"],
    status: "installed",
    source: "local",
    desc: "多散点 SDF 生成 HDA（蔓延参与判定）",
  },
  {
    id: "SimInspector",
    name: "SimInspector",
    origin: "D:\\001_Archive\\AI\\SimInspector",
    host: "UE",
    version: "",
    latest: "0.3.0",
    engines: [],
    status: "idle",
    source: "local",
    desc: "模拟属性 + 性能查看器",
  },
  {
    id: "MatHelper",
    name: "MatHelper",
    origin: "18163623522/MatHelper",
    host: "UE",
    version: "1.4.2",
    latest: "1.4.2",
    engines: [],
    status: "error",
    source: "github",
    desc: "数学工具库（上次构建失败：缺 4.26 工具链）",
  },
];

const filter = reactive<FilterState>({ ...DEFAULT_FILTER });

const filtered = computed(() => {
  const q = filter.search.trim().toLowerCase();
  return plugins.filter((p) => {
    if (filter.host !== "all" && p.host !== filter.host) return false;
    if (filter.status !== "all" && p.status !== filter.status) return false;
    if (filter.source !== "all" && p.source !== filter.source) return false;
    if (q) {
      const hay = `${p.name}\n${p.origin}\n${p.desc}`.toLowerCase();
      if (!hay.includes(q)) return false;
    }
    return true;
  });
});

function applyFilter(patch: Partial<FilterState>) {
  Object.assign(filter, patch);
}
function clearFilters() {
  Object.assign(filter, DEFAULT_FILTER);
}

/* 预览模式 no-op：Task 1.8 接 commands */
// eslint-disable-next-line @typescript-eslint/no-unused-vars
function noop(_id: string) {}
</script>

<template>
  <div class="page">
    <FilterBar :model-value="filter" @update:model-value="applyFilter" @add="noop('')" />
    <PluginList
      :plugins="filtered"
      @primary="noop"
      @uninstall="noop"
      @open-dir="noop"
      @clear-filters="clearFilters"
    />
    <div class="preview-note">布局预览 · 占位数据（真实数据在 Task 1.8 接入）</div>
  </div>
</template>

<style scoped>
.page {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  position: relative;
}
.preview-note {
  position: absolute;
  right: 18px;
  bottom: 10px;
  font-size: 10.5px;
  color: var(--text-sep);
  pointer-events: none;
}
</style>
