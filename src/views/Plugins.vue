<script setup lang="ts">
import { computed, onMounted, onUnmounted, reactive, ref } from "vue";
import { openPath } from "@tauri-apps/plugin-opener";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import FilterBar from "../components/FilterBar.vue";
import PluginList from "../components/PluginList.vue";
import InstallDialog from "../components/InstallDialog.vue";
import ConfirmDialog from "../components/ConfirmDialog.vue";
import AddSourceDialog from "../components/AddSourceDialog.vue";
import InstallProgressDialog from "../components/InstallProgressDialog.vue";
import {
  call,
  inTauri,
  DEFAULT_FILTER,
  type CompatStatusDto,
  type EnginePreflight,
  type EngineRow,
  type FilterState,
  type InstallResult,
  type PluginRow,
  type PreflightItem,
  type TargetPath,
  type UeEngine,
  type UninstallReport,
  type UpdateDto,
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
const engines = ref<EngineRow[]>([]);
const filter = reactive<FilterState>({ ...DEFAULT_FILTER });

const toast = ref<{ msg: string; tone: "ok" | "err" | "warn" } | null>(null);
let toastTimer: number | undefined;
function showToast(msg: string, tone: "ok" | "err" | "warn" = "ok") {
  toast.value = { msg, tone };
  clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => (toast.value = null), 4500);
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
  // 后台补全兼容矩阵（卡片徽标条）；不阻塞列表显示
  loadCompat();
}

async function loadCompat() {
  if (!inTauri) return;
  try {
    const all = await call<{ id: string; compat: { engine: string; status: CompatStatusDto }[] }[]>(
      "compat_all",
    );
    const byId = new Map(all.map((a) => [a.id, a.compat]));
    rows.value = rows.value.map((r) => {
      const list = byId.get(r.id);
      if (!list) return r;
      const map: Record<string, CompatStatusDto> = {};
      for (const c of list) map[c.engine] = c.status;
      return { ...r, compat: map };
    });
  } catch {
    /* 兼容矩阵失败不打扰列表 */
  }
}

async function loadEngines() {
  if (!inTauri) return;
  try {
    const dto = await call<{
      ue: UeEngine[];
      houdini: { version: string; root: string }[];
      obsidian: { vaults: { id: string; name: string; path: string }[] };
    }>("detect_engines");
    // UE / Houdini / Obsidian vault 统一成 EngineRow（vault 的安装标识 = id）
    allEngines.value = {
      UE: dto.ue.map((e) => ({ version: e.version, root: e.root })),
      Houdini: dto.houdini.map((h) => ({ version: h.version, root: h.root })),
      Obsidian: dto.obsidian.vaults.map((v) => ({ id: v.id, version: v.name, root: v.path })),
    };
    engines.value = allEngines.value.UE;
  } catch (e) {
    showToast(`引擎检测失败：${e}`, "err");
  }
}
const allEngines = ref<{ UE: EngineRow[]; Houdini: EngineRow[]; Obsidian: EngineRow[] }>({
  UE: [],
  Houdini: [],
  Obsidian: [],
});

/** 引擎筛选 chip 的标签全集（检测到的 UE 版本 + Houdini 版本 + vault 名） */
const engineLabels = computed(() => [
  ...allEngines.value.UE.map((e) => e.version),
  ...allEngines.value.Houdini.map((h) => h.version),
  ...allEngines.value.Obsidian.map((v) => v.version),
]);

/** 更新检查：远端新版本 → 黄点 + latest 字段 */
async function checkUpdates() {
  if (!inTauri) return;
  try {
    const updates = await call<UpdateDto[]>("check_updates");
    rows.value = rows.value.map((r) => {
      const u = updates.find((x) => x.id === r.id);
      if (!u || u.state.kind !== "available") return { ...r, status: r.status === "updatable" ? "installed" : r.status };
      const tag = u.state.newVersion;
      return {
        ...r,
        status: r.status === "idle" ? "idle" : "updatable",
        latest: tag === "目录内容已变化" ? r.version : tag,
      };
    });
  } catch {
    /* 更新检查失败不打扰列表 */
  }
}

let unlistenLog: UnlistenFn | null = null;
onMounted(() => {
  refresh();
  loadEngines();
  if (inTauri) {
    listen<string>("install-log", (e) => {
      liveLog.value.push(e.payload);
      // 长构建（RunUAT/pnpm）日志无上限会撑内存——滑窗保留尾部
      if (liveLog.value.length > 800) {
        liveLog.value.splice(0, liveLog.value.length - 800);
      }
    }).then((u) => (unlistenLog = u));
  }
});
onUnmounted(() => unlistenLog?.());

const filtered = computed(() => {
  const q = filter.search.trim().toLowerCase();
  return rows.value.filter((p) => {
    if (filter.host !== "all" && p.host !== filter.host) return false;
    if (filter.status !== "all" && p.status !== filter.status) return false;
    if (filter.source !== "all" && p.source !== filter.source) return false;
    if (filter.engine !== "all" && !p.engines.includes(filter.engine)) return false;
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

/* ———— 添加源 ———— */
const addDlg = ref(false);
const addBusy = ref(false);

function openAdd() {
  liveLog.value = [];
  addDlg.value = true;
}

async function addLocal(path: string) {
  addBusy.value = true;
  try {
    const added = await call<PluginRow>("add_local_source", { path });
    addDlg.value = false;
    await refresh();
    await checkUpdates();
    showToast(`已添加 ${added.name}（${added.host}）`, "ok");
  } catch (e) {
    showToast(`添加失败：${e}`, "err");
  }
  addBusy.value = false;
}

async function addGit(url: string) {
  addBusy.value = true;
  try {
    const added = await call<PluginRow>("add_git_source", { url });
    addDlg.value = false;
    await refresh();
    await checkUpdates();
    showToast(`已添加 ${added.name}（GitHub 源）`, "ok");
  } catch (e) {
    showToast(`添加失败：${e}`, "err");
    liveLog.value.push(`✗ ${e}`);
  }
  addBusy.value = false;
}

/* ———— 安装（compat 徽标 + 预检门禁 + 进度日志浮层 + 可取消）———— */
const installDlg = ref(false);
const installTarget = ref<PluginRow | null>(null);
const installCompat = ref<Record<string, CompatStatusDto>>({});
const installPreflight = ref<Record<string, PreflightItem[]>>({});
const progressDlg = ref(false);
const progressDone = ref(false);
const installToken = ref("");
/** install-log 事件统一落这里：添加源与安装进度共用，打开时清空 */
const liveLog = ref<string[]>([]);

async function askInstall(id: string) {
  const p = rows.value.find((r) => r.id === id);
  if (!p) return;
  installTarget.value = p;
  installCompat.value = {};
  installPreflight.value = {};
  // 引擎列表按宿主切换（Houdini = 检测到的安装；Obsidian = 检测到的 vault）
  if (allEngines.value[p.host].length === 0) await loadEngines();
  engines.value = allEngines.value[p.host];

  // git 源 UE 插件：拉兼容矩阵 + 五项预检（联网，可能几秒）
  if (inTauri && p.source === "github" && p.host === "UE") {
    try {
      const compat = await call<{ engine: string; status: CompatStatusDto }[]>("compat_for", { id });
      const map: Record<string, CompatStatusDto> = {};
      for (const c of compat) map[c.engine] = c.status;
      installCompat.value = map;
    } catch (e) {
      showToast(`兼容矩阵查询失败：${e}`, "warn");
    }
    try {
      const pf = await call<EnginePreflight[]>("preflight_for", { id });
      const map: Record<string, PreflightItem[]> = {};
      for (const e of pf) map[e.engine] = e.items;
      installPreflight.value = map;
    } catch {
      /* 预检失败不拦安装（后端还会再查一次） */
    }
  }
  installDlg.value = true;
}

async function doInstall(selected: string[]) {
  installDlg.value = false;
  const p = installTarget.value;
  if (!p) return;
  installToken.value = crypto.randomUUID();
  liveLog.value = [];
  progressDone.value = false;
  progressDlg.value = true;

  // compat 给的分支 ref → 传给后端 checkout
  const refs: Record<string, string> = {};
  for (const [engine, s] of Object.entries(installCompat.value)) {
    if (s.kind === "installable" && s.gitRef) refs[engine] = s.gitRef;
  }

  try {
    const results = await call<InstallResult[]>("install_local", {
      id: p.id,
      engines: selected,
      refs: Object.keys(refs).length > 0 ? refs : null,
      compat: Object.keys(installCompat.value).length > 0 ? installCompat.value : null,
      token: installToken.value,
    });
    const ok = results.filter((r) => r.ok);
    const bad = results.filter((r) => !r.ok);
    if (bad.length === 0) {
      showToast(`已安装到 ${ok.map((r) => r.engine).join("、")}`, "ok");
    } else {
      showToast(
        `成功 ${ok.length} / 失败 ${bad.length}：${bad[0].error ?? ""}`,
        ok.length ? "warn" : "err",
      );
    }
  } catch (e) {
    showToast(`安装失败：${e}`, "err");
  }
  progressDone.value = true;
  await refresh();
}

async function cancelInstall() {
  if (!installToken.value) return;
  try {
    const killed = await call<boolean>("cancel_build", { token: installToken.value });
    if (!killed) liveLog.value.push("（无进行中的构建进程——Release/拷贝阶段无法中断）");
  } catch (e) {
    liveLog.value.push(`取消失败：${e}`);
  }
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
    else showToast("GitHub 源无本地目录（缓存目录见设置）", "warn");
  } catch {
    showToast("打开目录失败", "err");
  }
}
</script>

<template>
  <div class="page">
    <FilterBar :model-value="filter" :engines="engineLabels" @update:model-value="applyFilter" @add="openAdd" />
    <PluginList
      :plugins="filtered"
      @primary="askInstall"
      @uninstall="askUninstall"
      @open-dir="openDir"
      @clear-filters="clearFilters"
    />

    <AddSourceDialog
      v-model="liveLog"
      v-model:busy="addBusy"
      :visible="addDlg"
      @add-local="addLocal"
      @add-git="addGit"
      @cancel="addDlg = false"
    />
    <InstallDialog
      :visible="installDlg"
      :plugin="installTarget"
      :engines="engines"
      :compat="installCompat"
      :preflight="installPreflight"
      @confirm="doInstall"
      @cancel="installDlg = false"
    />
    <InstallProgressDialog
      :visible="progressDlg"
      :title="`安装 ${installTarget?.name ?? ''}`"
      :lines="liveLog"
      :cancellable="true"
      :done="progressDone"
      @cancel="cancelInstall"
      @close="progressDlg = false"
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
