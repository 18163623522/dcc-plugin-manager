<script setup lang="ts">
// 引擎页 —— Task 1.2/1.3 后填充：detect_ue_engines / detect_houdini 结果
</script>

<template>
  <div class="page">
    <div class="empty">
      <div class="empty-title">引擎检测</div>
      <div class="empty-sub">Task 1.2 / 1.3 接入后显示本机 UE 与 Houdini 列表</div>
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
