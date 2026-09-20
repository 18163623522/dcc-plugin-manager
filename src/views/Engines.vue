<script setup lang="ts">
import { onMounted, ref } from "vue";
import { call, inTauri, type EnginesDto } from "../api";

const data = ref<EnginesDto | null>(null);
const error = ref("");
const loading = ref(false);

async function refresh() {
  if (!inTauri) return;
  loading.value = true;
  error.value = "";
  try {
    data.value = await call<EnginesDto>("detect_engines");
  } catch (e) {
    error.value = String(e);
  }
  loading.value = false;
}

onMounted(refresh);
</script>

<template>
  <div class="page">
    <div class="head">
      <div class="summary">
        <template v-if="data">
          <span class="num">{{ data.ue.length }}</span> 个 Unreal Engine
          <span class="sep">·</span>
          <span class="num">{{ data.houdini.length }}</span> 个 Houdini
          <span class="sep">·</span>
          <span class="num">{{ data.obsidian.vaults.length }}</span> 个 Obsidian vault
          <template v-if="data.obsidian.appVersion">（应用 {{ data.obsidian.appVersion }}）</template>
        </template>
        <template v-else-if="!inTauri">浏览器预览模式：引擎检测需在应用内运行</template>
      </div>
      <button v-if="inTauri" class="refresh" :disabled="loading" @click="refresh">
        {{ loading ? "检测中…" : "重新检测" }}
      </button>
    </div>

    <div v-if="error" class="error">{{ error }}</div>

    <div class="scroll">
      <section v-if="data && data.ue.length">
        <h3>Unreal Engine<span class="tag">LauncherInstalled.dat + RunUAT 校验</span></h3>
        <div class="grid">
          <div v-for="e in data.ue" :key="e.root" class="engine-card">
            <div class="row1">
              <span class="ver mono">{{ e.version }}</span>
              <span class="ok-badge">✓ RunUAT</span>
            </div>
            <div class="path" :title="e.root">{{ e.root }}</div>
          </div>
        </div>
      </section>

      <section v-if="data && data.houdini.length">
        <h3>Houdini<span class="tag">注册表扫描 + packages 目录推导</span></h3>
        <div class="grid">
          <div v-for="h in data.houdini" :key="h.root" class="engine-card">
            <div class="row1">
              <span class="ver mono">{{ h.version }}</span>
            </div>
            <div class="path" :title="h.root">{{ h.root }}</div>
            <div class="path sub" :title="h.packagesDir">packages: {{ h.packagesDir }}</div>
          </div>
        </div>
      </section>

      <section v-if="data && data.obsidian.vaults.length">
        <h3>Obsidian<span class="tag">obsidian.json vault 枚举</span></h3>
        <div class="grid">
          <div v-for="v in data.obsidian.vaults" :key="v.id" class="engine-card">
            <div class="row1">
              <span class="ver">{{ v.name }}</span>
              <span v-if="v.open" class="ok-badge">当前打开</span>
            </div>
            <div class="path" :title="v.path">{{ v.path }}</div>
          </div>
        </div>
      </section>
    </div>
  </div>
</template>

<style scoped>
.page {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.head {
  flex: none;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 18px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.05);
}
.summary {
  font-size: 12px;
  color: var(--text-2);
}
.summary .num {
  font-size: 14px;
}
.sep {
  color: var(--text-sep);
  margin: 0 8px;
}

.refresh {
  height: 28px;
  padding: 0 12px;
  border: none;
  border-radius: var(--radius-ctrl);
  background: rgba(255, 255, 255, 0.07);
  color: var(--text-1);
  font-family: var(--font-stack);
  font-size: 12px;
  cursor: pointer;
}
.refresh:hover {
  background: var(--btn-hover);
}

.error {
  margin: 12px 18px 0;
  padding: 10px 12px;
  border-radius: var(--radius-ctrl);
  background: rgba(255, 69, 58, 0.1);
  border: 1px solid rgba(255, 69, 58, 0.3);
  color: #ff8a80;
  font-size: 12px;
}

.scroll {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 4px 18px 26px;
}

section {
  margin-top: 14px;
}
h3 {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-1);
  display: flex;
  align-items: baseline;
  gap: 10px;
}
.tag {
  font-size: 10.5px;
  font-weight: 400;
  color: var(--text-sep);
}

.grid {
  margin-top: 10px;
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
  gap: 10px;
}

.engine-card {
  padding: 12px 14px;
  border-radius: var(--radius-pop);
  background: rgba(255, 255, 255, 0.035);
  border: 1px solid rgba(255, 255, 255, 0.06);
}
.row1 {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.ver {
  font-size: 16px;
  font-weight: 600;
  color: var(--text-num);
}
.ok-badge {
  font-size: 10.5px;
  color: var(--c-ok);
  background: rgba(48, 209, 88, 0.12);
  padding: 2px 8px;
  border-radius: 8px;
}
.path {
  margin-top: 7px;
  font-size: 10.5px;
  font-family: var(--font-mono);
  color: var(--text-2);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.path.sub {
  color: var(--text-sep);
}
</style>
