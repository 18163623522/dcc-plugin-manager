<script setup lang="ts">
import { onMounted, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import {
  call,
  inTauri,
  type CacheStats,
  type EnvStatus,
  type Settings,
} from "../api";

const mb = (b: number) => `${(b / 1048576).toFixed(1)} MB`;

const cache = ref<CacheStats | null>(null);
const env = ref<EnvStatus | null>(null);
const roots = ref<string[]>([]);
const busy = ref("");
const theme = ref<"default" | "winui3">("default");

/** 主题切换：html[data-theme] 驱动 tokens.css 双主题变量组 */
function applyTheme(t: "default" | "winui3") {
  document.documentElement.dataset.theme = t;
}

async function setTheme(t: "default" | "winui3") {
  if (theme.value === t) return;
  theme.value = t;
  applyTheme(t);
  await save();
}

const toast = ref<{ msg: string; tone: "ok" | "err" } | null>(null);
let timer: number | undefined;
function showToast(msg: string, tone: "ok" | "err" = "ok") {
  toast.value = { msg, tone };
  clearTimeout(timer);
  timer = window.setTimeout(() => (toast.value = null), 4000);
}

async function refresh() {
  if (!inTauri) return;
  try {
    cache.value = await call<CacheStats>("cache_stats");
    env.value = await call<EnvStatus>("env_status");
    const s = await call<Settings>("get_settings");
    roots.value = s.extraUeRoots;
    theme.value = s.theme === "winui3" ? "winui3" : "default";
    applyTheme(theme.value);
  } catch (e) {
    showToast(`读取设置失败：${e}`, "err");
  }
}
onMounted(refresh);

async function clearCache(kind: "repos" | "releases" | "build") {
  busy.value = kind;
  try {
    const freed = await call<number>("clear_cache", {
      repos: kind === "repos",
      releases: kind === "releases",
      build: kind === "build",
    });
    showToast(`已清理，释放 ${mb(freed)}`, "ok");
    await refresh();
  } catch (e) {
    showToast(`清理失败：${e}`, "err");
  }
  busy.value = "";
}

async function addRoot() {
  const dir = await open({ directory: true, title: "选择引擎根目录（含 Engine\\Build\\BatchFiles\\RunUAT.bat）" });
  if (typeof dir !== "string" || !dir) return;
  if (!roots.value.includes(dir)) roots.value.push(dir);
  await save();
}

async function removeRoot(dir: string) {
  roots.value = roots.value.filter((r) => r !== dir);
  await save();
}

async function save() {
  try {
    await call("save_settings", { roots: roots.value, theme: theme.value });
    showToast("已保存（引擎页重新检测后生效）", "ok");
  } catch (e) {
    showToast(`保存失败：${e}`, "err");
  }
}
</script>

<template>
  <div class="page">
    <section>
      <h3>主题</h3>
      <div class="row">
        <span class="k">界面风格</span>
        <div class="seg">
          <button class="seg-btn" :class="{ active: theme === 'default' }" @click="setTheme('default')">原版</button>
          <button class="seg-btn" :class="{ active: theme === 'winui3' }" @click="setTheme('winui3')">WinUI 3</button>
        </div>
      </div>
      <div class="note">
        原版：obs-float-bar 观感（软蓝操作钮、无控件描边）；
        WinUI 3：Fluent 控件层（SystemAccentColor 色阶、按钮四态描边、输入框焦点条）。即时生效并记忆。
      </div>
    </section>

    <section>
      <h3>缓存</h3>
      <div class="row" v-if="cache">
        <span class="k">git 仓库</span>
        <span class="v mono">{{ mb(cache.repos) }}</span>
        <button class="btn" :disabled="busy === 'repos' || cache.repos === 0" @click="clearCache('repos')">
          清理
        </button>
      </div>
      <div class="row" v-if="cache">
        <span class="k">Release 下载</span>
        <span class="v mono">{{ mb(cache.releases) }}</span>
        <button class="btn" :disabled="busy === 'releases' || cache.releases === 0" @click="clearCache('releases')">
          清理
        </button>
      </div>
      <div class="row" v-if="cache">
        <span class="k">构建产物</span>
        <span class="v mono">{{ mb(cache.build) }}</span>
        <button class="btn" :disabled="busy === 'build' || cache.build === 0" @click="clearCache('build')">
          清理
        </button>
      </div>
      <div class="note">位于 %APPDATA%\dcc-plugin-manager\cache\；清理 git 仓库后重装需重新 clone，本地源不受影响。</div>
    </section>

    <section>
      <h3>自定义 UE 引擎根目录</h3>
      <div v-for="r in roots" :key="r" class="row">
        <span class="v mono wide">{{ r }}</span>
        <button class="btn" @click="removeRoot(r)">移除</button>
      </div>
      <div v-if="roots.length === 0" class="note">无（Launcher 安装自动检测；源码引擎在此追加）</div>
      <button class="btn add" @click="addRoot">＋ 添加根目录</button>
    </section>

    <section>
      <h3>环境</h3>
      <div class="row" v-if="env">
        <span class="k">gh CLI</span>
        <span class="badge" :class="env.gh ? 'ok' : 'bad'">{{ env.gh ? "可用（私仓/免限流）" : "未安装（匿名 REST 回退）" }}</span>
      </div>
      <div class="row" v-if="env">
        <span class="k">curl</span>
        <span class="badge" :class="env.curl ? 'ok' : 'bad'">{{ env.curl ? "可用" : "缺失" }}</span>
      </div>
      <div class="row" v-if="env">
        <span class="k">pnpm</span>
        <span class="badge" :class="env.pnpm ? 'ok' : 'bad'">{{ env.pnpm ? "可用（Obsidian 源码构建）" : "未安装" }}</span>
      </div>
    </section>

    <Transition name="toast">
      <div v-if="toast" class="toast" :class="toast.tone">{{ toast.msg }}</div>
    </Transition>
  </div>
</template>

<style scoped>
.page {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 16px 18px 26px;
  position: relative;
}

section {
  margin-bottom: 24px;
}
h3 {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-1);
  margin-bottom: 10px;
}

.row {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 6px 0;
}
.k {
  flex: none;
  width: 110px;
  font-size: 11.5px;
  color: var(--text-2);
}
.v {
  font-size: 11.5px;
  color: var(--text-num);
}
.v.wide {
  flex: 1;
  word-break: break-all;
}

.btn {
  height: 26px;
  padding: 0 11px;
  border: none;
  border-radius: var(--radius-ctrl);
  background: rgba(255, 255, 255, 0.07);
  color: var(--text-1);
  font-family: var(--font-stack);
  font-size: 11.5px;
  cursor: pointer;
  flex: none;
}
.btn:hover:not(:disabled) {
  background: var(--btn-hover);
}
.btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
.btn.add {
  margin-top: 6px;
  background: var(--btn-accent-bg);
  border-color: var(--btn-accent-border);
  color: var(--btn-accent-fg);
}
.btn.add:hover:not(:disabled) {
  background: var(--btn-accent-bg-hover);
  color: var(--btn-accent-fg-hover);
}

.badge {
  font-size: 11px;
  padding: 2px 9px;
  border-radius: 9px;
}
.badge.ok {
  background: rgba(48, 209, 88, 0.12);
  color: var(--c-ok);
}
.badge.bad {
  background: rgba(255, 69, 58, 0.12);
  color: var(--c-err);
}

.note {
  font-size: 11px;
  color: var(--text-2);
  margin-top: 6px;
  line-height: 1.6;
}

.toast {
  position: fixed;
  left: 50%;
  bottom: 18px;
  transform: translateX(-50%);
  padding: 9px 16px;
  border-radius: 12px;
  background: var(--bg-pop);
  border: 1px solid rgba(255, 255, 255, 0.1);
  font-size: 12px;
  color: var(--text-1);
  z-index: 60;
}
.toast.err {
  border-color: rgba(255, 69, 58, 0.45);
}

/* 主题分段选择（WinUI Selector / ToggleButton 分段形态） */
.seg {
  display: flex;
  gap: 4px;
  padding: 3px;
  border-radius: var(--radius-ctrl);
  background: var(--bg-sunken);
}
.seg-btn {
  height: 26px;
  padding: 0 14px;
  border: none;
  border-radius: 3px;
  background: transparent;
  color: var(--text-2);
  font-family: var(--font-stack);
  font-size: 12px;
  cursor: pointer;
  transition: background 0.15s, color 0.15s;
}
.seg-btn:hover {
  color: var(--text-1);
}
.seg-btn.active {
  background: var(--btn-accent-bg);
  color: var(--btn-accent-fg);
}
.toast-enter-active,
.toast-leave-active {
  transition: opacity 0.2s;
}
.toast-enter-from,
.toast-leave-to {
  opacity: 0;
}
</style>
