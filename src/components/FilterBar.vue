<script setup lang="ts">
import type { FilterState } from "../types";

const props = defineProps<{ modelValue: FilterState }>();
const emit = defineEmits<{ "update:modelValue": [patch: Partial<FilterState>]; add: [] }>();

interface ChipGroup {
  key: "host" | "status" | "source";
  label: string;
  options: { value: string; label: string }[];
}

const groups: ChipGroup[] = [
  {
    key: "host",
    label: "宿主",
    options: [
      { value: "all", label: "全部" },
      { value: "UE", label: "UE" },
      { value: "Houdini", label: "Houdini" },
    ],
  },
  {
    key: "status",
    label: "状态",
    options: [
      { value: "all", label: "全部" },
      { value: "installed", label: "已安装" },
      { value: "updatable", label: "可更新" },
      { value: "idle", label: "未安装" },
      { value: "error", label: "失败" },
    ],
  },
  {
    key: "source",
    label: "来源",
    options: [
      { value: "all", label: "全部" },
      { value: "github", label: "GitHub" },
      { value: "local", label: "本地" },
    ],
  },
];

function pick(group: ChipGroup, value: string) {
  emit("update:modelValue", { [group.key]: value } as Partial<FilterState>);
}

function onSearch(e: Event) {
  emit("update:modelValue", { search: (e.target as HTMLInputElement).value });
}
</script>

<template>
  <div class="filter-bar">
    <div class="groups">
      <div v-for="g in groups" :key="g.key" class="group">
        <span class="group-label">{{ g.label }}</span>
        <button
          v-for="opt in g.options"
          :key="opt.value"
          class="chip"
          :class="{ active: (modelValue as any)[g.key] === opt.value }"
          @click="pick(g, opt.value)"
        >
          {{ opt.label }}
        </button>
      </div>
    </div>

    <div class="right">
      <label class="search">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round">
          <circle cx="11" cy="11" r="7" />
          <path d="M20 20l-3.5-3.5" />
        </svg>
        <input
          :value="modelValue.search"
          type="text"
          placeholder="搜索名称 / 仓库 / 描述"
          spellcheck="false"
          @input="onSearch"
        />
      </label>
      <button class="primary-btn" @click="emit('add')">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round">
          <path d="M12 5v14M5 12h14" />
        </svg>
        添加源
      </button>
    </div>
  </div>
</template>

<style scoped>
.filter-bar {
  flex: none;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 10px 18px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.05);
}

.groups {
  display: flex;
  align-items: center;
  gap: 18px;
  min-width: 0;
  overflow: hidden;
}

.group {
  display: flex;
  align-items: center;
  gap: 4px;
}

.group-label {
  font-size: 11px;
  color: var(--text-2);
  margin-right: 4px;
  white-space: nowrap;
}

.chip {
  height: 24px;
  padding: 0 10px;
  border: none;
  border-radius: 12px;
  background: transparent;
  color: var(--text-2);
  font-family: var(--font-stack);
  font-size: 12px;
  cursor: pointer;
  white-space: nowrap;
  transition: background 0.12s, color 0.12s;
}
.chip:hover {
  background: rgba(255, 255, 255, 0.06);
  color: var(--text-1);
}
.chip.active {
  background: rgba(255, 255, 255, 0.14);
  color: var(--text-1);
}

.right {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: none;
}

.search {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 10px;
  border-radius: var(--radius-ctrl);
  background: var(--bg-sunken);
  border: 1px solid rgba(255, 255, 255, 0.06);
  transition: border-color 0.15s;
}
.search:focus-within {
  border-color: rgba(10, 132, 255, 0.55);
}
.search svg {
  width: 14px;
  height: 14px;
  color: var(--text-2);
  flex: none;
}
.search input {
  width: 168px;
  border: none;
  outline: none;
  background: transparent;
  color: var(--text-1);
  font-family: var(--font-stack);
  font-size: 12px;
}
.search input::placeholder {
  color: var(--text-2);
}

.primary-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 12px;
  border: none;
  border-radius: var(--radius-ctrl);
  background: var(--c-primary);
  color: #fff;
  font-family: var(--font-stack);
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: filter 0.12s, transform 0.12s;
}
.primary-btn svg {
  width: 13px;
  height: 13px;
}
.primary-btn:hover {
  filter: brightness(1.12);
}
.primary-btn:active {
  transform: scale(0.96);
}
</style>
