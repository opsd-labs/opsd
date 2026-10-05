<script setup lang="ts">
import { resourceLabels } from "../../resourcePresentation";
import { computed } from "vue";
import { workspaceKey } from "../../workspace";
import { inject } from "vue";
import Drawer from "../ui/Drawer.vue";
import Button from "../ui/Button.vue";
import Input from "../ui/Input.vue";
import Select from "../ui/Select.vue";
import Checkbox from "../ui/Checkbox.vue";
import Notice from "../ui/Notice.vue";
import Field from "../resources/Field.vue";
import ResourceDetails from "../resources/ResourceDetails.vue";
import ChangeDiff from "../resources/ChangeDiff.vue";
const w = inject(workspaceKey)!;
const titles: Record<string, string> = {
  node: "添加节点",
  stack: "导入已有 Stack",
  "create-stack": "创建 Stack",
  rule: "防火墙规则",
  image: "拉取镜像",
  compose: "更新 Compose",
  detail: "资源详情",
  external: "外部规则 · 只读",
  plan: "变更计划",
  stop: "停止容器",
  restart: "重启容器",
  "stack-control": "管理 Stack",
};
const isForm = computed(() =>
  ["node", "stack", "create-stack", "rule", "image", "compose"].includes(
    w.modal,
  ),
);
async function containerAction() {
  if (
    await w.dispatch(w.selected.node_id, {
      type: "docker",
      container: w.selected.id,
      operation: w.modal,
    })
  )
    w.close();
}
async function stackAction(operation: string) {
  if (
    await w.dispatch(w.selected.node_id, {
      type: "stack_control",
      project: w.selected.project,
      operation,
    })
  )
    w.close();
}
const ownedContainers = computed(() =>
  w.containers.filter(
    (c) =>
      c.node_id === w.selected?.node_id && c.project === w.selected?.project,
  ),
);
</script>
<template>
  <Drawer
    :open="!!w.modal"
    :title="titles[w.modal] || '资源操作'"
    :wide="['plan', 'compose', 'create-stack'].includes(w.modal)"
    @update:open="!$event && w.close()"
    ><Notice v-if="w.error" tone="danger">{{ w.error }}</Notice
    ><Notice v-if="w.demo">演示 · 不执行操作</Notice>
    <form
      v-if="isForm"
      id="operation-form"
      class="operation-form"
      @submit.prevent="w.save"
    >
      <template v-if="w.modal === 'node'"
        ><Field label="节点名称"
          ><Input
            v-model="w.form.name"
            required
            placeholder="C101 · 香港" /></Field
        ><Field
          label="公网地址"
          help="多个地址用逗号分隔；按实际登记，不以连接来源代替。"
          ><Input v-model="w.form.public_addresses" /></Field
        ><Field label="EasyTier 地址"
          ><Input v-model="w.form.overlay_address" /></Field
        ><Field label="SSH 端口"
          ><Input
            v-model="w.form.ssh_port"
            type="number"
            min="1"
            max="65535"
            required /></Field
        ><Notice>一次性注册令牌有效期十分钟。</Notice
        ><ResourceDetails :omit="['action','digest','key']"
          :labels="resourceLabels"
          v-if="w.token"
          :value="w.token" /></template
      ><template v-else-if="w.modal === 'stack'"
        ><Notice>核对原文件和工作目录；导入不会重建现有容器。</Notice
        ><Field label="原项目名"
          ><Input v-model="w.form.project" required /></Field
        ><Field label="原工作目录"
          ><Input
            v-model="w.form.directory"
            required
            placeholder="/opt/stacks/vaultwarden" /></Field
        ><Field label="Compose 文件" help="多文件按原合并顺序用逗号分隔。"
          ><Input v-model="w.form.files" required /></Field
        ><Field label="环境文件"
          ><Input v-model="w.form.env_files" /></Field></template
      ><template v-else-if="w.modal === 'image'"
        ><Field label="镜像引用"
          ><Input
            v-model="w.form.reference"
            required
            placeholder="nginx:stable" /></Field></template
      ><template v-else-if="w.modal === 'rule'"
        ><div class="form-grid">
          <Field label="地址族"
            ><Select
              v-model="w.form.family"
              :options="[
                { value: 4, label: 'IPv4' },
                { value: 6, label: 'IPv6' },
              ]" /></Field
          ><Field label="方向"
            ><Select
              v-model="w.form.direction"
              :options="[
                { value: 'input', label: '入站' },
                { value: 'output', label: '出站' },
                { value: 'forward', label: '转发' },
              ]"
          /></Field>
        </div>
        <Field label="来源 CIDR"
          ><Input v-model="w.form.source" placeholder="留空为任意来源" /></Field
        ><Field label="目标 CIDR"
          ><Input v-model="w.form.destination" placeholder="留空为任意目标"
        /></Field>
        <div class="form-grid">
          <Field label="协议"
            ><Select
              v-model="w.form.protocol"
              :options="
                ['tcp', 'udp', 'icmp', 'icmpv6', 'any'].map((value) => ({
                  value,
                  label: value === 'any' ? '任意' : value.toUpperCase(),
                }))
              " /></Field
          ><Field label="目标端口"
            ><Input v-model="w.form.ports" placeholder="80,443"
          /></Field>
        </div>
        <Field label="动作"
          ><Select
            v-model="w.form.action"
            :options="[
              { value: 'accept', label: '允许' },
              { value: 'drop', label: '丢弃' },
              { value: 'reject', label: '拒绝' },
            ]" /></Field
        ><Checkbox v-model="w.form.enabled">启用规则</Checkbox
        ><Notice
          >提交生成计划，实际发布仍受节点能力与验证限制。</Notice
        ></template
      ><template v-else
        ><Field v-if="w.modal === 'create-stack'" label="项目名称"
          ><Input
            v-model="w.form.project"
            required
            pattern="[a-z0-9][a-z0-9_-]*"
        /></Field>
        <p v-else class="form-context">{{ w.selected.node_name }} · {{ w.selected.project }}</p>
        <Field
          label="Compose 配置"
          help="提供完整配置；先校验和查看差异，再发布。"
          ><Input
            v-model="w.form.content"
            multiline
            rows="18"
            class="mono"
            required
            spellcheck="false" /></Field
      ></template>
    </form>
    <template v-else-if="w.modal === 'plan'"
      ><Notice
        >目标：{{
          w.nodes.find((n) => n.node.id === w.selected.node_id)?.node.name ||
          w.selected.node_id
        }}</Notice
      ><ChangeDiff
        :labels="resourceLabels"
        :value="w.selected.result" /></template
    ><template v-else-if="w.modal === 'stop' || w.modal === 'restart'"
      ><ResourceDetails :omit="['action','digest','key']"
        :labels="resourceLabels"
        :value="{
          name: w.selected.names?.[0],
          node_name: w.selected.node_name,
          protected: w.selected.protected,
        }"
      /><Notice tone="warning"
        >此操作会影响容器正在提供的服务。</Notice
      ></template
    ><template v-else-if="w.modal === 'stack-control'"
      ><ResourceDetails :omit="['action','digest','key']"
        :labels="resourceLabels"
        :value="{
          project: w.selected.project,
          node_name: w.selected.node_name,
          directory: w.selected.directory,
        }"
      />
      <h3 class="section-heading">受影响容器与挂载</h3>
      <ResourceDetails :omit="['action','digest','key']"
        :labels="resourceLabels"
        :value="
          ownedContainers.map((c) => ({ name: c.names?.[0], mounts: c.mounts }))
        "
      /><Notice tone="warning"
        >移除项目将删除项目容器，保留命名卷和宿主机目录。</Notice
      ></template
    ><template v-else-if="w.modal === 'external'"
      ><Notice>原始规则未结构化，不能据此声称没有外部规则；保持只读。</Notice>
      <pre class="external-pre">{{
        typeof w.selected === "string"
          ? w.selected
          : JSON.stringify(w.selected, null, 2)
      }}</pre></template
    ><ResourceDetails :omit="['action','digest','key']"
      :labels="resourceLabels"
      v-else
      :value="w.selected"
    /><template #footer
      ><Button @click="w.close">关闭</Button
      ><Button
        v-if="isForm"
        variant="primary"
        type="submit"
        form="operation-form"
        :disabled="w.busy || !!w.token"
        :busy="w.busy"
        >{{
          w.modal === "node"
            ? "生成注册凭据"
            : ["rule", "compose", "create-stack"].includes(w.modal)
              ? "生成计划"
              : "提交任务"
        }}</Button
      ><Button
        v-else-if="w.modal === 'plan'"
        variant="primary"
        :disabled="w.busy || w.demo"
        @click="w.executePlan(w.selected)"
        >发布此计划</Button
      ><Button
        v-else-if="w.modal === 'stop' || w.modal === 'restart'"
        variant="danger"
        :disabled="w.busy"
        @click="containerAction"
        >{{ w.modal === "stop" ? "停止容器" : "重启容器" }}</Button
      ><template v-else-if="w.modal === 'stack-control'"
        ><Button :disabled="w.busy" @click="stackAction('stop')"
          >停止 Stack</Button
        ><Button
          variant="danger"
          :disabled="w.busy"
          @click="stackAction('remove')"
          >移除并保留数据</Button
        ></template
      ></template
    ></Drawer
  >
</template>

<style scoped>
.operation-form {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.form-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 16px;
}
.form-context {
  font-size: 13px;
  color: var(--muted);
  margin: 0;
}
.section-heading {
  font-size: 13px;
  font-weight: 700;
  margin: 16px 0 8px;
}
.external-pre {
  font-family: "JetBrains Mono", "Fira Code", monospace;
  font-size: 12px;
  line-height: 1.5;
  background: var(--bg);
  padding: 16px;
  overflow: auto;
  max-height: 400px;
  white-space: pre-wrap;
  word-break: break-all;
}
</style>
