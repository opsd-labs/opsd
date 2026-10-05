<script setup lang="ts">
import { inject } from "vue";
import { workspaceKey } from "../workspace";
import Button from "../components/ui/Button.vue";
import Notice from "../components/ui/Notice.vue";
import SectionNumber from "../components/ui/SectionNumber.vue";

const w = inject(workspaceKey)!;
</script>

<template>
  <div class="storage">
    <div class="storage-header">
      <SectionNumber num="06" />
      <div>
        <h2 class="storage-title">存储部署</h2>
        <p class="storage-sub">三阶段工作流：预检 → 准备 → 部署</p>
      </div>
    </div>

    <Notice tone="info">
      存储部署采用三阶段工作流，确保每一步都经过验证。
    </Notice>

    <div class="storage-steps">
      <div class="storage-step">
        <div class="storage-step-num">01</div>
        <h3 class="storage-step-title">预检</h3>
        <p class="storage-step-desc">检查磁盘状态与可用性</p>
        <Button :disabled="!w.node?.connected" @click="w.open('storage-probe')">开始预检</Button>
      </div>
      <div class="storage-step">
        <div class="storage-step-num">02</div>
        <h3 class="storage-step-title">准备</h3>
        <p class="storage-step-desc">格式化磁盘并创建卷组</p>
        <Button :disabled="!w.node?.connected" @click="w.open('storage-prepare')">开始准备</Button>
      </div>
      <div class="storage-step">
        <div class="storage-step-num">03</div>
        <h3 class="storage-step-title">部署</h3>
        <p class="storage-step-desc">部署存储集群并初始化</p>
        <Button :disabled="!w.node?.connected" @click="w.open('storage-deploy')">开始部署</Button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.storage {
  display: flex;
  flex-direction: column;
  gap: 24px;
}
.storage-header {
  display: flex;
  align-items: flex-start;
  gap: 24px;
}
.storage-title {
  font-size: 24px;
  font-weight: 700;
  line-height: 1.2;
  margin: 0 0 4px;
}
.storage-sub {
  font-size: 13px;
  color: var(--muted);
  margin: 0;
}
.storage-steps {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 24px;
  padding: 24px 0;
}
.storage-step {
  padding: 24px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--r);
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.storage-step-num {
  font-size: 48px;
  font-weight: 700;
  line-height: 1;
  color: var(--line);
  letter-spacing: -0.02em;
}
.storage-step-title {
  font-size: 16px;
  font-weight: 700;
  margin: 0;
}
.storage-step-desc {
  font-size: 13px;
  color: var(--muted);
  margin: 0;
  flex: 1;
}
@media (max-width: 1000px) {
  .storage-steps {
    grid-template-columns: 1fr;
  }
}
</style>
