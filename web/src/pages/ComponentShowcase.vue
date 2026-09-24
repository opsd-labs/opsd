<script setup lang="ts">
import { ref } from "vue";
import Button from "../components/ui/Button.vue";
import Input from "../components/ui/Input.vue";
import Select from "../components/ui/Select.vue";
import Checkbox from "../components/ui/Checkbox.vue";
import Status from "../components/ui/Status.vue";
import Notice from "../components/ui/Notice.vue";
import Drawer from "../components/ui/Drawer.vue";
import Menu from "../components/ui/Menu.vue";
import DataTable from "../components/resources/DataTable.vue";
import Toolbar from "../components/resources/Toolbar.vue";
import Field from "../components/resources/Field.vue";
import { useWorkspace } from "../workspace";
const w = useWorkspace(),
  value = ref(""),
  checked = ref(true),
  open = ref(false),
  state = ref("data");
const rows = Array.from({ length: 60 }, (_, i) => ({
  id: i,
  name: `资源 ${i + 1}`,
  address: "2001:db8:1234:5678:abcd:ef01:2345:6789",
}));
</script>
<template>
  <Toolbar
    ><Select
      v-model="w.theme"
      aria-label="样板主题"
      :options="[
        { value: 'light', label: '亮色' },
        { value: 'dark', label: '深色' },
      ]" /><Button variant="primary">主要操作</Button
    ><Button disabled>不可用</Button><Button busy disabled>正在提交…</Button
    ><Button @click="open = true">打开抽屉</Button
    ><Menu
      :items="[
        { id: 'a', label: '菜单操作' },
        { id: 'b', label: '不可用操作', disabled: true },
      ]" /></Toolbar
  ><Toolbar
    ><Status tone="success">在线</Status><Status tone="warning">待同步</Status
    ><Status tone="danger">失败</Status
    ><Checkbox v-model="checked">选项</Checkbox
    ><Input
      v-model="value"
      aria-label="筛选样板"
      placeholder="筛选样板" /></Toolbar
  ><Notice tone="danger">错误：此处展示完整操作错误，不用颜色替代文字。</Notice
  ><Select
    v-model="state"
    aria-label="样板数据状态"
    :options="[
      { value: 'data', label: '有数据' },
      { value: 'loading', label: '加载中' },
      { value: 'error', label: '读取失败' },
      { value: 'unknown', label: '未采集' },
      { value: 'empty', label: '空数据' },
    ]"
  /><DataTable
    :rows="state === 'empty' ? [] : rows"
    :columns="[
      { key: 'name', label: '名称', sortable: true, width: 180 },
      { key: 'address', label: '长 IPv6', mono: true, width: 440 },
    ]"
    row-key="id"
    label="组件样板表格"
    :search="value"
    :loading="state === 'loading'"
    :unknown="state === 'unknown'"
    :error="state === 'error' ? '模拟读取错误' : undefined"
  /><Drawer v-model:open="open" title="抽屉样板"
    ><Field label="字段"><Input v-model="value" /></Field
    ><template #footer
      ><Button @click="open = false">关闭</Button
      ><Button variant="primary">提交</Button></template
    ></Drawer
  >
</template>
