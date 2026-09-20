<script setup lang="ts">
// 插件列表页 —— Task 1.8 填充：列表 + 分类搜索 + 添加本地源
</script>

<template>
  <div class="page">
    <div class="empty">
      <div class="empty-title">暂无插件</div>
      <div class="empty-sub">从右上角「添加源」导入本地插件目录或 GitHub 仓库</div>
    </div>
  </div>
</template>

<style scoped>
.page {
  flex: 1;
  display: grid;
  place-items: center;
}
.empty {
  text-align: center;
}
.empty-title {
  font-size: 14px;
  color: var(--text-1);
  font-weight: 600;
}
.empty-sub {
  font-size: 12px;
  color: var(--text-2);
  margin-top: 6px;
}
</style>
