<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import FilterBar from "../components/FilterBar.vue";
import PluginList from "../components/PluginList.vue";
import InstallDialog from "../components/InstallDialog.vue";
import ConfirmDialog from "../components/ConfirmDialog.vue";
import {
  call,
  inTauri,
  DEFAULT_FILTER,
  type FilterState,
  type InstallResult,
  type PluginRow,
  type TargetPath,
  type UeEngine,
  type UninstallReport,
} from "../api";

/* ———— 浏览器预览模式的占位数据（真实数据走 command）———— */
const MOCK: PluginRow[] = [
  { id: "TrueGlow", name: "TrueGlow", host: "UE", version: "0.8.2", latest: "0.9.0", engines: ["4.26", "5.7", "5.8"], status: "updatable", source: "github", origin: "18163623522/TrueGlow", desc: "物理辉光：HDR 金字塔 Bloom + Streak + 星芒" },
  { id: "BetterHLSL", name: "BetterHLSL", host: "UE", version: "2.1.0", latest: "2.1.0", engines: ["4.26", "5.7"], status: "installed", source: "github", origin: "18163623522/BetterHLSL", desc: "HLSL 补全对标 Visual Studio" },
  { id: "HoudiniGraphTools", name: "HoudiniGraphTools", host: "UE", version: "2.0.0", latest: "2.0.0", engines: ["4.26"], status: "installed", source: "github", origin: "18163623522/houdini-graph-tools", desc: "摇晃断线修复 + L 键自动排布" },
  { id: "MultiPointSDF", name: "多点 SDF", host: "Houdini", version: "1.1.0", latest: "1.1.0", engines: ["20.0"], status: "installed", source: "local", origin: "D:\\...\\MultiPointSDF\\otls", desc: "多散点 SDF 生成 HDA（蔓延参与判定）" },
  { id: "SimInspector", name: "SimInspector", host: "UE", version: "", latest: "0.3.0", engines: [], status: "idle", source: "local", origin: "D:\\001_Archive\\AI\\SimInspector", desc: "模拟属性 + 性能查看器" },
  { id: "MatHelper", name: "MatHelper", host: "UE", version: "1.4.2", latest: "1.4.2", engines: [], status: "error", source: "github", origin: "18163623522/MatHelper", desc: "数学工具库（上次构建失败：缺 4.26 工具链）" },
];

const rows = ref<PluginRow[]>([]);
const engines = ref<UeEngine[]>([]);
const filter = reactive<FilterState>({ ...DEFAULT_FILTER });

const toast = ref<{ msg: string; tone: "ok" | "err" | "warn" } | null>(null);
let toastTimer: number | undefined;
function showToast(msg: string, tone: "ok" | "err" | "warn" = "ok") {
  toast.value = { msg, tone };
  clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => (toast.value = null), 4000);
}

async function refresh() {
  if (!inTauri) {
    rows.value = MOCK;
    return;
  }
  try {
    rows.value = await call<PluginRow[]>("list_plugins");
  } catch (e) {
    showToast(`读取插件列表失败：${e}`, "err");
  }
}

async function loadEngines() {
  if (!inTauri) return;
  try {
    engines.value = (await call<{ ue: UeEngine[] }>("detect_engines")).ue;
  } catch (e) {
    showToast(`引擎检测失败：${e}`, "err");
  }
}

onMounted(() => {
  refresh();
  loadEngines();
});

const filtered = computed(() => {
  const q = filter.search.trim().toLowerCase();
  return rows.value.filter((p) => {
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

/* ———— 添加本地源 ———— */
async function addSource() {
  if (!inTauri) return showToast("浏览器预览模式：请在应用内添加源", "warn");
  try {
    const dir = await open({ directory: true, title: "选择插件目录（含 .uplugin 或 Houdini 包）" });
    if (!dir || typeof dir !== "string") return;
    const added = await call<PluginRow>("add_local_source", { path: dir });
    await refresh();
    showToast(`已添加 ${added.name}（${added.host}）`, "ok");
  } catch (e) {
    showToast(`添加失败：${e}`, "err");
  }
}

/* ———— 安装 ———— */
const installDlg = ref(false);
const installTarget = ref<PluginRow | null>(null);
function askInstall(id: string) {
  const p = rows.value.find((r) => r.id === id);
  if (!p) return;
  if (p.host === "Houdini") return showToast("Houdini 安装在里程碑 4 提供", "warn");
  installTarget.value = p;
  installDlg.value = true;
}
async function doInstall(selected: string[]) {
  installDlg.value = false;
  const p = installTarget.value;
  if (!p) return;
  try {
    const results = await call<InstallResult[]>("install_local", { id: p.id, engines: selected });
    const ok = results.filter((r) => r.ok);
    const bad = results.filter((r) => !r.ok);
    if (bad.length === 0) {
      showToast(`已安装到 ${ok.map((r) => r.engine).join("、")}`, "ok");
    } else {
      showToast(`成功 ${ok.length} / 失败 ${bad.length}：${bad[0].error ?? ""}`, bad.length && ok.length ? "warn" : "err");
    }
  } catch (e) {
    showToast(`安装失败：${e}`, "err");
  }
  await refresh();
}

/* ———— 卸载 ———— */
const uninstallDlg = ref(false);
const uninstallTarget = ref<PluginRow | null>(null);
const uninstallPaths = ref<TargetPath[]>([]);
const uninstallBusy = ref(false);
async function askUninstall(id: string) {
  const p = rows.value.find((r) => r.id === id);
  if (!p) return;
  try {
    const paths = await call<TargetPath[]>("plan_uninstall", { id, engines: [] });
    if (paths.length === 0) return showToast("没有可卸载的安装", "warn");
    uninstallTarget.value = p;
    uninstallPaths.value = paths;
    uninstallDlg.value = true;
  } catch (e) {
    showToast(`卸载预览失败：${e}`, "err");
  }
}
async function doUninstall() {
  const p = uninstallTarget.value;
  if (!p) return;
  uninstallBusy.value = true;
  try {
    const report = await call<UninstallReport>("do_uninstall", { id: p.id, engines: [] });
    if (report.locked.length > 0) {
      showToast(`${report.locked[0].path} 被占用——请关闭对应引擎后重试`, "err");
    } else if (report.failed.length > 0) {
      showToast(`卸载失败：${report.failed[0].reason}`, "err");
    } else {
      const n = report.removed.length + report.missing.length;
      showToast(`已卸载 ${n} 处安装`, "ok");
    }
  } catch (e) {
    showToast(`卸载失败：${e}`, "err");
  }
  uninstallBusy.value = false;
  uninstallDlg.value = false;
  await refresh();
}

/* ———— 打开目录 ———— */
async function openDir(id: string) {
  const p = rows.value.find((r) => r.id === id);
  if (!p || !inTauri) return;
  try {
    if (p.engines.length > 0) {
      const paths = await call<TargetPath[]>("plan_uninstall", { id, engines: [] });
      if (paths[0]) return void (await openPath(paths[0].path));
    }
    if (p.source === "local") await openPath(p.origin);
    else showToast("GitHub 源无本地目录", "warn");
  } catch {
    showToast("打开目录失败", "err");
  }
}
</script>

<template>
  <div class="page">
    <FilterBar :model-value="filter" @update:model-value="applyFilter" @add="addSource" />
    <PluginList
      :plugins="filtered"
      @primary="askInstall"
      @uninstall="askUninstall"
      @open-dir="openDir"
      @clear-filters="clearFilters"
    />

    <InstallDialog
      :visible="installDlg"
      :plugin="installTarget"
      :engines="engines"
      @confirm="doInstall"
      @cancel="installDlg = false"
    />
    <ConfirmDialog
      :visible="uninstallDlg"
      :title="`卸载 ${uninstallTarget?.name ?? ''}`"
      :paths="uninstallPaths"
      :busy="uninstallBusy"
      @confirm="doUninstall"
      @cancel="uninstallDlg = false"
    />

    <Transition name="toast">
      <div v-if="toast" class="toast" :class="toast.tone">{{ toast.msg }}</div>
    </Transition>
    <div v-if="!inTauri" class="preview-note">布局预览 · 占位数据（应用内为真实数据）</div>
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

.toast {
  position: absolute;
  left: 50%;
  bottom: 18px;
  transform: translateX(-50%);
  max-width: 76%;
  padding: 9px 16px;
  border-radius: 12px;
  background-color: var(--bg-pop);
  background-image: var(--noise);
  background-blend-mode: overlay;
  backdrop-filter: blur(20px) saturate(var(--shell-saturate));
  border: 1px solid rgba(255, 255, 255, 0.1);
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
  font-size: 12px;
  color: var(--text-1);
  z-index: 60;
  animation: field-in 0.2s ease-out;
}
.toast.err {
  border-color: rgba(255, 69, 58, 0.45);
}
.toast.warn {
  border-color: rgba(255, 214, 10, 0.4);
}
.toast-enter-active,
.toast-leave-active {
  transition: opacity 0.2s, transform 0.2s;
}
.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(6px);
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
