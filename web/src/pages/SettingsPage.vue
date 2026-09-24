<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useWorkspace } from "../workspace";
import { api, basePath } from "../api";
import Field from "../components/resources/Field.vue";
import Select from "../components/ui/Select.vue";
import Button from "../components/ui/Button.vue";
import Input from "../components/ui/Input.vue";
import Notice from "../components/ui/Notice.vue";
import Status from "../components/ui/Status.vue";
import ShareSettings from "../components/settings/ShareSettings.vue";
import ThemeSettings from "../components/settings/ThemeSettings.vue";
import AuditSettings from "../components/settings/AuditSettings.vue";
const w = useWorkspace();
const current = ref(basePath().replace(/^\//, ""));
const draft = ref("");
const error = ref("");
const notice = ref("");
const busy = ref(false);
const valid = computed(() => draft.value.length === 16 && draft.value !== current.value);
/** 入口是路径第一段；这里只展示地址形状，不泄露到日志或错误信息。 */
const url = computed(
  () => `${location.origin}/${current.value}/`,
);
async function save() {
  busy.value = true;
  error.value = "";
  notice.value = "";
  try {
    if (w.demo) {
      notice.value = "演示模式：未提交设置变更";
      return;
    }
    const result = await api("/settings/entrance", "PUT", { value: draft.value });
    if (result.changed) {
      notice.value = `安全入口已更新；旧地址立即失效，60 秒宽限后需重新登录。新地址：${location.origin}/${result.value}/`;
      // 宽限期结束后再跳转，让当前会话平滑过渡。
      setTimeout(() => {
        location.href = `/${result.value}/`;
      }, 60000);
    } else notice.value = "安全入口未变化";
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    busy.value = false;
  }
}
onMounted(async () => {
  if (w.demo) return;
  try {
    const result = await api("/settings/entrance");
    current.value = result.value;
    draft.value = result.value;
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  }
});
</script>
<template>
  <section class="settings-section">
    <h2>外观</h2>
    <Field label="主题"
      ><Select
        v-model="w.theme"
        :options="[
          { value: 'system', label: '跟随系统' },
          { value: 'light', label: '亮色' },
          { value: 'dark', label: '柔和深色' },
        ]"
    /></Field>
    <p class="muted">中性底 + 局部靛蓝 · 13px 正文 · 36px 表格行高</p>
  </section>
  <section class="settings-section">
    <h2>安全入口</h2>
    <Field label="当前地址"
      ><span class="mono">{{ url }}</span></Field
    >
    <Field label="访问规则">
      <Status tone="neutral" mark="blocked">仅入口路径可访问</Status>
      <span class="muted"
        >不带安全入口的请求会被直接丢弃，不返回任何响应。</span
      >
    </Field>
    <Field label="修改入口">
      <Input
        v-model="draft"
        :maxlength="16"
        aria-label="新的安全入口"
        placeholder="16 位字母、数字与 - . _ ~"
      /><Button :disabled="!valid || busy" @click="save">应用新入口</Button>
    </Field>
    <Notice v-if="error" tone="danger">{{ error }}</Notice>
    <Notice v-if="notice">{{ notice }}</Notice>
    <p class="muted">
      安全入口不是认证，只是降低被扫描到的概率。会话 Cookie、CSRF 与来源校验仍然生效。
      修改后旧地址立即失效，当前会话有 60 秒宽限；请务必保存新地址，否则将无法再进入控制台。
    </p>
  </section>
  <ThemeSettings />
  <AuditSettings />
  <ShareSettings />
  <section class="settings-section">
    <h2>备份与恢复</h2>
    <p>通过主控 CLI 创建一致性备份。恢复前停止原主控，使用全新控制库。</p>
    <p class="muted">
      备份包含控制记录、CA 和身份密钥；部署配置需一同保存。安全入口、分享令牌与 API Key 随控制记录一起备份。
    </p>
  </section>
</template>
