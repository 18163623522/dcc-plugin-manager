<script setup lang="ts">
import { computed, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { call, type RepoBrief } from "../api";

defineProps<{ visible: boolean }>();
const emit = defineEmits<{
  addLocal: [path: string];
  addGit: [url: string];
  /** 批量导入我名下的 GitHub 仓库 */
  importRepos: [urls: string[]];
  cancel: [];
}>();

const mode = ref<"local" | "git">("local");
const url = ref("");
const pickedDir = ref("");
/** add 过程日志（install-log 事件由父级灌进来） */
const logLines = defineModel<string[]>({ default: () => [] });
const busy = defineModel<boolean>("busy", { default: false });

/* —— 我的 GitHub 仓库导入 —— */
const repos = ref<RepoBrief[] | null>(null);
const reposLoading = ref(false);
const reposErr = ref("");
const picked = ref<Set<string>>(new Set());

async function loadRepos() {
  if (repos.value || reposLoading.value) return;
  reposLoading.value = true;
  reposErr.value = "";
  try {
    repos.value = await call<RepoBrief[]>("github_my_repos");
  } catch (e) {
    reposErr.value = String(e);
  }
  reposLoading.value = false;
}

function toggleRepo(full: string) {
  const next = new Set(picked.value);
  if (next.has(full)) next.delete(full);
  else next.add(full);
  picked.value = next;
}

const primaryText = computed(() =>
  busy.value
    ? "处理中…"
    : mode.value === "git" && picked.value.size > 0
      ? `导入（${picked.value.size}）`
      : "添加",
);
const primaryDisabled = computed(
  () =>
    busy.value ||
    (mode.value === "local" ? !pickedDir.value : picked.value.size === 0 && !url.value.trim()),
);

function onPrimary() {
  if (busy.value) return;
  if (mode.value === "git" && picked.value.size > 0) {
    emit("importRepos", [...picked.value].map((n) => `https://github.com/${n}`));
  } else {
    confirm();
  }
}

async function pickFolder() {
  const dir = await open({ directory: true, title: "选择插件目录（含 .uplugin 或 Houdini 包）" });
  if (typeof dir === "string") pickedDir.value = dir;
}

function confirm() {
  if (busy.value) return;
  if (mode.value === "local" && pickedDir.value) emit("addLocal", pickedDir.value);
  else if (mode.value === "git" && url.value.trim()) emit("addGit", url.value.trim());
}
</script>

<template>
  <Teleport to="body">
    <div v-if="visible" class="overlay" @click.self="!busy && emit('cancel')">
      <div class="dialog">
        <div class="title">添加源</div>

        <div class="tabs">
          <button class="tab" :class="{ active: mode === 'local' }" @click="mode = 'local'">本地目录</button>
          <button class="tab" :class="{ active: mode === 'git' }" @click="mode = 'git'">GitHub 仓库</button>
        </div>

        <div v-if="mode === 'local'" class="body">
          <button class="pick-btn" @click="pickFolder">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round">
              <path d="M4 7a2 2 0 0 1 2-2h4l2 2h6a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2z" />
            </svg>
            选择插件目录
          </button>
          <div v-if="pickedDir" class="picked mono">{{ pickedDir }}</div>
          <div class="hint">识别规则：深度 ≤2 内找 .uplugin → UE；otls/hda/hda\*.dll → Houdini</div>
        </div>

        <div v-else class="body">
          <input
            v-model="url"
            class="url-input mono"
            type="text"
            placeholder="https://github.com/<owner>/<repo>"
            spellcheck="false"
            :disabled="busy"
            @keyup.enter="confirm"
          />
          <div class="hint">clone 到本地缓存（已登录 gh 免配置；Release 附件优先安装，无附件走源码构建）</div>

          <button class="pick-btn import" :disabled="busy || reposLoading" @click="loadRepos">
            {{ reposLoading ? "读取仓库列表中…" : "从我的 GitHub 导入仓库" }}
          </button>
          <div v-if="reposErr" class="picked">{{ reposErr }}</div>
          <div v-if="repos" class="repo-list">
            <label
              v-for="r in repos"
              :key="r.fullName"
              class="repo"
              :class="{ on: picked.has(r.fullName) }"
            >
              <input type="checkbox" :checked="picked.has(r.fullName)" @change="toggleRepo(r.fullName)" />
              <span class="rname mono">{{ r.fullName }}</span>
              <span class="rdesc" :title="r.description ?? ''">{{ r.description ?? "" }}</span>
            </label>
            <div v-if="repos.length === 0" class="hint">名下没有非 fork 仓库</div>
          </div>
          <div v-if="picked.size" class="hint">已选 {{ picked.size }} 个，点下方按钮批量导入（识别不了的仓库会跳过并报告）</div>
        </div>

        <div v-if="logLines.length" class="log mono">
          <div v-for="(l, i) in logLines" :key="i" class="log-line">{{ l }}</div>
        </div>

        <div class="footer">
          <button class="btn" :disabled="busy" @click="emit('cancel')">取消</button>
          <button class="btn primary" :disabled="primaryDisabled" @click="onPrimary">
            {{ primaryText }}
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

.tabs {
  display: flex;
  gap: 4px;
  margin-top: 14px;
  padding: 3px;
  border-radius: var(--radius-ctrl);
  background: var(--bg-sunken);
}
.tab {
  flex: 1;
  height: 28px;
  border: none;
  border-radius: 3px;
  background: transparent;
  color: var(--text-2);
  font-family: var(--font-stack);
  font-size: 12px;
  cursor: pointer;
  transition: background 0.15s, color 0.15s;
}
.tab.active {
  background: rgba(255, 255, 255, 0.12);
  color: var(--text-1);
}

.body {
  margin-top: 14px;
  min-height: 64px;
}

.pick-btn {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 34px;
  padding: 0 14px;
  border: 1px dashed rgba(255, 255, 255, 0.2);
  border-radius: var(--radius-ctrl);
  background: transparent;
  color: var(--text-1);
  font-family: var(--font-stack);
  font-size: 12.5px;
  cursor: pointer;
  transition: border-color 0.15s, background 0.15s;
}
.pick-btn:hover {
  border-color: var(--accent-text);
  background: var(--accent-soft-bg);
}
.pick-btn.import {
  width: 100%;
  justify-content: center;
  margin-top: 10px;
  border-style: solid;
}

/* 我的仓库多选列表 */
.repo-list {
  margin-top: 10px;
  max-height: 180px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.repo {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 10px;
  border: 1px solid var(--tsep, rgba(255, 255, 255, 0.1));
  border-color: var(--stroke-ctrl);
  border-radius: var(--radius-ctrl);
  cursor: pointer;
  transition: background 0.12s, border-color 0.12s;
}
.repo:hover {
  background: var(--btn-hover);
}
.repo.on {
  background: var(--accent-soft-bg);
  border-color: var(--accent-soft-border);
}
.repo input {
  accent-color: var(--accent-fill);
  flex: none;
}
.rname {
  font-size: 11.5px;
  color: var(--text-1);
  flex: none;
  font-weight: 600;
}
.rdesc {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 10.5px;
  color: var(--text-2);
}
.pick-btn svg {
  width: 15px;
  height: 15px;
  color: var(--text-2);
}

.picked {
  margin-top: 9px;
  font-size: 11px;
  color: var(--text-num);
  word-break: break-all;
}

.url-input {
  width: 100%;
  height: 34px;
  padding: 0 12px;
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: var(--radius-ctrl);
  background: var(--bg-sunken);
  color: var(--text-1);
  font-family: var(--font-mono);
  font-size: 12px;
  outline: none;
  transition: border-color 0.15s;
}
.url-input:focus {
  border-color: var(--accent-text);
  box-shadow: var(--focus-underline, none);
}
.url-input::placeholder {
  color: var(--text-sep);
}

.hint {
  margin-top: 9px;
  font-size: 10.5px;
  color: var(--text-sep);
}

.log {
  margin-top: 12px;
  max-height: 110px;
  overflow-y: auto;
  padding: 8px 10px;
  border-radius: var(--radius-ctrl);
  background: var(--bg-sunken);
  font-size: 10.5px;
  color: var(--text-2);
  line-height: 1.7;
}

.footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 16px;
}

/* Standard 按钮：ControlFill 6% + ControlStroke 7%，hover 提亮、pressed 减淡 */
.btn {
  height: 30px;
  padding: 0 14px;
  border: 1px solid var(--stroke-ctrl);
  border-radius: var(--radius-ctrl);
  background: var(--btn-rest);
  color: var(--text-1);
  font-family: var(--font-stack);
  font-size: 12px;
  cursor: pointer;
  transition: background 0.12s, border-color 0.12s;
}
.btn:hover {
  background: var(--btn-hover);
  border-color: var(--stroke-ctrl);
}
.btn.primary {
  background: var(--btn-accent-bg);
  border-color: var(--btn-accent-border);
  color: var(--btn-accent-fg);
}
.btn.primary:hover {
  background: var(--btn-accent-bg-hover);
  color: var(--btn-accent-fg-hover);
}
.btn.primary:active {
  background: var(--btn-accent-bg-active);
}
.btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}
</style>
