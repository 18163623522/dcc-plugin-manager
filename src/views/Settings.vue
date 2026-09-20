<script setup lang="ts">
// 设置页 —— M2+ 填充：缓存目录、gh 路径、清理缓存
</script>

<template>
  <div class="page">
    <div class="empty">
      <div class="empty-title">设置</div>
      <div class="empty-sub">缓存目录 / gh 路径 / 清理缓存（后续里程碑开放）</div>
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
