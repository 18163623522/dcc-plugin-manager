<script setup lang="ts">
import { inTauri } from "../api";
</script>

<template>
  <div class="page">
    <div class="section">
      <h3>数据与缓存</h3>
      <div class="kv">
        <span class="k">注册表</span>
        <span class="v mono">%APPDATA%\dcc-plugin-manager\registry.json</span>
      </div>
      <div class="kv">
        <span class="k">插件缓存</span>
        <span class="v mono">%APPDATA%\dcc-plugin-manager\cache\repos\</span>
      </div>
      <div class="note">git 仓库缓存（GitHub 源）在里程碑 2 接入；清理缓存在此处提供。</div>
    </div>

    <div class="section">
      <h3>引擎根目录</h3>
      <div class="note">
        自定义引擎根目录追加（源码引擎等非 Launcher 安装）随里程碑 2 设置项提供；
        当前检测来源：LauncherInstalled.dat + Houdini 注册表。
      </div>
    </div>

    <div v-if="!inTauri" class="note dim">浏览器预览模式：设置项需在应用内生效。</div>
  </div>
</template>

<style scoped>
.page {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 16px 18px 26px;
}

.section {
  margin-bottom: 22px;
}
h3 {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-1);
  margin-bottom: 10px;
}

.kv {
  display: flex;
  align-items: baseline;
  gap: 14px;
  padding: 7px 0;
}
.k {
  flex: none;
  width: 70px;
  font-size: 11.5px;
  color: var(--text-2);
}
.v {
  font-size: 11px;
  color: var(--text-num);
  word-break: break-all;
}

.note {
  font-size: 11px;
  color: var(--text-2);
  line-height: 1.6;
}
.note.dim {
  color: var(--text-sep);
}
</style>
