<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useWorkspace } from "../../workspace";
import { api } from "../../api";
import Field from "../resources/Field.vue";
import Button from "../ui/Button.vue";
import Checkbox from "../ui/Checkbox.vue";
import Input from "../ui/Input.vue";
import Select from "../ui/Select.vue";
import Notice from "../ui/Notice.vue";
import Status from "../ui/Status.vue";

/**
 * 分享面管理：开关、站点信息、分享令牌与 API Key。
 *
 * 令牌与密钥的**明文只在创建响应里出现一次**，之后控制库只保留摘要，
 * 因此这里把新产生的值单独显著展示，并要求管理员自行保存。
 */
const w = useWorkspace();
const enabled = ref(false);
const site = ref({ name: "", description: "", footer: "" });
const tokens = ref<any[]>([]);
const keys = ref<any[]>([]);
const error = ref("");
const notice = ref("");
const busy = ref(false);
const freshToken = ref("");
const freshKey = ref("");
const tokenForm = ref({ label: "", expires_hours: "24", scopeAll: true, nodeText: "" });
const keyForm = ref({ label: "", scopeAll: true, nodeText: "" });

/** 分享页的地址形状；令牌本身在创建时才知道。 */
const shareBase = computed(() => `${location.origin}/share/`);

/** 逗号分隔的节点编号，空串表示不限定。 */
function parseNodes(text: string): string[] | null {
  const list = text
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);
  return list.length ? list : null;
}
const liveTokens = computed(() => tokens.value.filter((t) => !t.revoked));
const liveKeys = computed(() => keys.value.filter((k) => !k.revoked));

async function guard(fn: () => Promise<void>) {
  busy.value = true;
  error.value = "";
  try {
    await fn();
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    busy.value = false;
  }
}

async function load() {
  if (w.demo) {
    notice.value = "演示模式：分享面配置未从服务器读取";
    return;
  }
  await guard(async () => {
    const [s, t, k] = await Promise.all([
      api("/share/settings"),
      api("/share/tokens"),
      api("/share/keys"),
    ]);
    enabled.value = s.enabled;
    site.value = s.site;
    tokens.value = t.tokens;
    keys.value = k.keys;
  });
}

async function saveSettings() {
  await guard(async () => {
    if (w.demo) {
      notice.value = "演示模式：未提交设置变更";
      return;
    }
    await api("/share/settings", "PUT", { enabled: enabled.value, site: site.value });
    notice.value = enabled.value ? "分享面已开启" : "分享面已关闭";
  });
}

async function createToken() {
  await guard(async () => {
    if (w.demo) {
      notice.value = "演示模式：未创建分享令牌";
      return;
    }
    const result = await api("/share/tokens", "POST", {
      label: tokenForm.value.label || "未命名分享",
      expires_hours: Number(tokenForm.value.expires_hours) || null,
      nodes: tokenForm.value.scopeAll ? null : parseNodes(tokenForm.value.nodeText),
    });
    freshToken.value = result.token;
    freshKey.value = "";
    tokenForm.value.label = "";
    await load();
  });
}

async function createKey() {
  await guard(async () => {
    if (w.demo) {
      notice.value = "演示模式：未创建 API Key";
      return;
    }
    const result = await api("/share/keys", "POST", {
      label: keyForm.value.label || "未命名密钥",
      nodes: keyForm.value.scopeAll ? null : parseNodes(keyForm.value.nodeText),
    });
    freshKey.value = result.key;
    freshToken.value = "";
    keyForm.value.label = "";
    await load();
  });
}

async function revoke(path: string, id: string) {
  await guard(async () => {
    if (w.demo) return;
    await api(`/share/${path}/${id}`, "DELETE");
    await load();
  });
}

function expiryText(token: any) {
  if (!token.expires_at) return "长期有效";
  const seconds = token.expires_at * 1000 - Date.now();
  if (seconds <= 0) return "已过期";
  const hours = Math.round(seconds / 3600000);
  return hours < 48 ? `${hours} 小时后到期` : `${Math.round(hours / 24)} 天后到期`;
}
function scopeText(record: any) {
  if (!record.nodes) return "全部可见节点";
  return `${record.nodes.length} 个指定节点`;
}
onMounted(load);
</script>
<template>
  <section class="settings-section">
    <h2>分享页</h2>
    <Field label="状态">
      <Checkbox v-model="enabled">对外可见</Checkbox>
      <Status :tone="enabled ? 'success' : 'neutral'" :mark="enabled ? 'active' : 'blocked'">{{
        enabled ? "分享面已开启" : "分享面已关闭"
      }}</Status>
    </Field>
    <Field label="站点名称"
      ><Input v-model="site.name" aria-label="站点名称" :maxlength="60"
    /></Field>
    <Field label="站点描述"
      ><Input v-model="site.description" aria-label="站点描述" :maxlength="200"
    /></Field>
    <Field label="页脚文字"
      ><Input
        v-model="site.footer"
        aria-label="页脚文字"
        :maxlength="200"
        placeholder="留空则显示默认说明"
    /></Field>
    <Field label="保存"><Button variant="primary" :disabled="busy" @click="saveSettings">应用</Button></Field>
    <Notice v-if="error" tone="danger">{{ error }}</Notice>
    <Notice v-if="notice">{{ notice }}</Notice>
    <p class="muted">
      分享页位于 <code class="mono">{{ shareBase }}&lt;令牌&gt;/</code>，<strong>不经过安全入口</strong>，
      使用自己的令牌。控制台会话 Cookie 的 Path 已限定在安全入口之下，浏览器不会把它发给分享路径，
      因此分享页拿不到任何控制台凭据。
    </p>
    <p class="muted">
      公开响应只包含字段白名单中的内容：显示名、地区、分组、标签、在线状态与主机指标。
      <strong>永不下发</strong>公网地址、EasyTier 地址、SSH 端口、容器与防火墙细节、任务与审计内容。
    </p>
  </section>

  <section class="settings-section">
    <h2>分享令牌</h2>
    <Field label="名称"
      ><Input v-model="tokenForm.label" aria-label="分享令牌名称" :maxlength="60" placeholder="例如：给同事看"
    /></Field>
    <Field label="有效时长"
      ><Select
        v-model="tokenForm.expires_hours"
        aria-label="分享令牌有效时长"
        :options="[
          { value: 1, label: '1 小时' },
          { value: 24, label: '1 天' },
          { value: 168, label: '7 天' },
          { value: 720, label: '30 天' },
          { value: 0, label: '长期有效' },
        ]"
    /></Field>
    <Field label="可见范围">
      <Checkbox v-model="tokenForm.scopeAll">全部可见节点</Checkbox>
      <Input
        v-if="!tokenForm.scopeAll"
        v-model="tokenForm.nodeText"
        aria-label="限定可见节点编号"
        placeholder="节点编号，逗号分隔，例如 C052,C071"
      />
    </Field>
    <Field label="创建"
      ><Button variant="primary" :disabled="busy" @click="createToken">生成令牌</Button></Field
    >
    <Notice v-if="freshToken" tone="warning">
      <span>
        新令牌（只显示这一次，请立即保存）：<code class="mono">{{ shareBase }}{{ freshToken }}/</code>
      </span>
    </Notice>
    <table v-if="liveTokens.length" class="settings-table">
      <thead>
        <tr>
          <th>名称</th>
          <th>范围</th>
          <th>有效期</th>
          <th>创建时间</th>
          <th>操作</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="token in liveTokens" :key="token.id">
          <td>{{ token.label }}</td>
          <td>{{ scopeText(token) }}</td>
          <td>{{ expiryText(token) }}</td>
          <td class="mono">
            {{ new Date(token.created_at * 1000).toLocaleString("zh-CN", { hour12: false }) }}
          </td>
          <td><Button variant="text" @click="revoke('tokens', token.id)">撤销</Button></td>
        </tr>
      </tbody>
    </table>
    <p v-else class="muted">还没有分享令牌。</p>
  </section>

  <section class="settings-section">
    <h2>API Key</h2>
    <p class="muted">
      供 Grafana 等面板消费 <code class="mono">/api/v1/public/metrics</code>（Prometheus 文本格式）。
      抓取需要<strong>分享令牌 + API Key 双重校验</strong>：令牌决定能读哪些节点，密钥决定是否有权以机器方式读取。
    </p>
    <Field label="名称"
      ><Input v-model="keyForm.label" aria-label="API Key 名称" :maxlength="60" placeholder="例如：Grafana"
    /></Field>
    <Field label="可见范围">
      <Checkbox v-model="keyForm.scopeAll">全部可见节点</Checkbox>
      <Input
        v-if="!keyForm.scopeAll"
        v-model="keyForm.nodeText"
        aria-label="限定密钥可见节点编号"
        placeholder="节点编号，逗号分隔，例如 C052,C071"
      />
    </Field>
    <Field label="创建"
      ><Button variant="primary" :disabled="busy" @click="createKey">生成密钥</Button></Field
    >
    <Notice v-if="freshKey" tone="warning">
      <span>新密钥（只显示这一次，请立即保存）：<code class="mono">{{ freshKey }}</code></span>
    </Notice>
    <table v-if="liveKeys.length" class="settings-table">
      <thead>
        <tr>
          <th>名称</th>
          <th>范围</th>
          <th>最近使用</th>
          <th>创建时间</th>
          <th>操作</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="key in liveKeys" :key="key.id">
          <td>{{ key.label }}</td>
          <td>{{ scopeText(key) }}</td>
          <td class="mono">
            {{
              key.last_used
                ? new Date(key.last_used * 1000).toLocaleString("zh-CN", { hour12: false })
                : "尚未使用"
            }}
          </td>
          <td class="mono">
            {{ new Date(key.created_at * 1000).toLocaleString("zh-CN", { hour12: false }) }}
          </td>
          <td><Button variant="text" @click="revoke('keys', key.id)">撤销</Button></td>
        </tr>
      </tbody>
    </table>
    <p v-else class="muted">还没有 API Key。</p>
  </section>
</template>
