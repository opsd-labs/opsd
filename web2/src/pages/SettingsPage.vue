<script setup lang="ts">
import { inject } from "vue";
import { workspaceKey } from "../workspace";
import Button from "../components/ui/Button.vue";
import SectionNumber from "../components/ui/SectionNumber.vue";
import Divider from "../components/ui/Divider.vue";

const w = inject(workspaceKey)!;
</script>

<template>
  <div class="settings">
    <div class="settings-header">
      <SectionNumber num="07" />
      <div>
        <h2 class="settings-title">设置</h2>
        <p class="settings-sub">控制台配置与偏好</p>
      </div>
    </div>

    <section class="settings-section">
      <h3 class="settings-section-title">外观</h3>
      <Divider />
      <div class="settings-row">
        <div>
          <div class="settings-label">主题</div>
          <div class="settings-desc">选择控制台的外观主题</div>
        </div>
        <div class="settings-control">
          <Button variant="text" @click="w.applyTheme('system')">系统</Button>
          <Button variant="text" @click="w.applyTheme('light')">浅色</Button>
          <Button variant="text" @click="w.applyTheme('dark')">深色</Button>
        </div>
      </div>
    </section>

    <section class="settings-section">
      <h3 class="settings-section-title">审计</h3>
      <Divider />
      <div class="settings-row">
        <div>
          <div class="settings-label">审计日志</div>
          <div class="settings-desc">查看所有操作记录</div>
        </div>
        <div class="settings-control">
          <Button @click="w.open('audit')">查看日志</Button>
        </div>
      </div>
    </section>

    <section class="settings-section">
      <h3 class="settings-section-title">分享</h3>
      <Divider />
      <div class="settings-row">
        <div>
          <div class="settings-label">分享链接</div>
          <div class="settings-desc">生成只读状态页面链接</div>
        </div>
        <div class="settings-control">
          <Button @click="w.open('share')">生成链接</Button>
        </div>
      </div>
    </section>

    <section class="settings-section">
      <h3 class="settings-section-title">关于</h3>
      <Divider />
      <div class="settings-row">
        <div>
          <div class="settings-label">版本</div>
          <div class="settings-desc">opsd 控制台 v0.2.0</div>
        </div>
      </div>
    </section>
  </div>
</template>

<style scoped>
.settings {
  display: flex;
  flex-direction: column;
  gap: 32px;
}
.settings-header {
  display: flex;
  align-items: flex-start;
  gap: 24px;
}
.settings-title {
  font-size: 24px;
  font-weight: 700;
  line-height: 1.2;
  margin: 0 0 4px;
}
.settings-sub {
  font-size: 13px;
  color: var(--muted);
  margin: 0;
}
.settings-section {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.settings-section-title {
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--muted);
  margin: 0;
}
.settings-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 24px;
  padding: 12px 0;
}
.settings-label {
  font-size: 13px;
  font-weight: 700;
  color: var(--ink);
  margin-bottom: 4px;
}
.settings-desc {
  font-size: 12px;
  color: var(--muted);
}
.settings-control {
  display: flex;
  gap: 8px;
}
</style>
