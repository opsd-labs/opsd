<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useWorkspace } from "../../workspace";
import { api, apiPath, getCsrf } from "../../api";
import Field from "../resources/Field.vue";
import Button from "../ui/Button.vue";
import Checkbox from "../ui/Checkbox.vue";
import Input from "../ui/Input.vue";
import Select from "../ui/Select.vue";
import Notice from "../ui/Notice.vue";
import Tag from "../ui/Tag.vue";

/**
 * 主题管理。两条主题线粒度不同，选择器也**相互独立**：
 *
 * - 控制台主题是**令牌级**的，只覆盖颜色、圆角、密度等变量，不替换组件结构；
 * - 分享页主题是**包级**的，会替换整个分享页前端。
 *
 * 一个主题可以同时声明支持两者，但两个选择器分别设置，互不牵连。
 */
const w = useWorkspace();
type Theme = {
  short: string;
  name: any;
  description?: any;
  version?: string | null;
  author?: any;
  surfaces: string[];
  has_assets: boolean;
  digest: string;
  fields: any[];
};
const themes = ref<Theme[]>([]);
const consoleActive = ref("default");
const shareActive = ref("default");
const consoleSettings = ref<Record<string, any>>({});
const shareSettings = ref<Record<string, any>>({});
const error = ref("");
const notice = ref("");
const busy = ref(false);
const manifestText = ref("");
const packageName = ref("");

const consoleThemes = computed(() =>
  themes.value.filter((t) => t.surfaces.includes("console")),
);
const shareThemes = computed(() =>
  themes.value.filter((t) => t.surfaces.includes("share")),
);
const bothThemes = computed(() =>
  themes.value.filter((t) => t.surfaces.length > 1),
);
const activeConsole = computed(() =>
  consoleThemes.value.find((t) => t.short === consoleActive.value),
);
const activeShare = computed(() =>
  shareThemes.value.find((t) => t.short === shareActive.value),
);

function label(value: any): string {
  if (!value) return "";
  if (typeof value === "string") return value;
  return value["zh-CN"] || value["zh"] || Object.values(value)[0] || "";
}

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
    notice.value = "演示模式：未读取主题列表";
    return;
  }
  await guard(async () => {
    const [list, active] = await Promise.all([
      api("/themes"),
      api("/themes/active"),
    ]);
    themes.value = list.themes;
    consoleActive.value = active.console?.short || "default";
    shareActive.value = active.share?.short || "default";
    consoleSettings.value = active.console?.settings || {};
    shareSettings.value = active.share?.settings || {};
  });
}

async function activate(surface: "console" | "share") {
  await guard(async () => {
    if (w.demo) {
      notice.value = "演示模式：未切换主题";
      return;
    }
    const short = surface === "console" ? consoleActive.value : shareActive.value;
    const settings = surface === "console" ? consoleSettings.value : shareSettings.value;
    await api("/themes/active", "PUT", { surface, short, settings });
    notice.value = `${surface === "console" ? "控制台" : "分享页"}主题已切换为 ${
      short === "default" ? "内置主题" : short
    }`;
    await load();
    // 控制台主题改的是 CSS 变量，立刻重新应用一次
    if (surface === "console") await applyConsoleTokens();
  });
}

async function removeTheme(short: string) {
  await guard(async () => {
    if (w.demo) return;
    await api(`/themes/${short}`, "DELETE");
    await load();
    await applyConsoleTokens();
  });
}

/** 安装控制台主题：提交清单 JSON。 */
async function installConsole() {
  await guard(async () => {
    if (w.demo) {
      notice.value = "演示模式：未安装主题";
      return;
    }
    let manifest: unknown;
    try {
      manifest = JSON.parse(manifestText.value);
    } catch {
      throw new Error("清单不是合法的 JSON");
    }
    await api("/themes/console", "POST", manifest);
    notice.value = "控制台主题已安装";
    manifestText.value = "";
    await load();
  });
}

/** 安装分享页主题：直接上传包字节，主控自行计算摘要。 */
async function installSharePackage(file: File) {
  await guard(async () => {
    if (w.demo) {
      notice.value = "演示模式：未上传主题包";
      return;
    }
    const buffer = await file.arrayBuffer();
    // 走与其他请求相同的前缀与 CSRF 处理，不在这里另建一套
    const response = await fetch(apiPath("/themes/share"), {
      method: "POST",
      headers: {
        "Content-Type": "application/zip",
        "X-CSRF-Token": getCsrf(),
      },
      body: buffer,
    });
    const data = await response.json().catch(() => ({}));
    if (!response.ok) throw new Error(data.error || `上传失败：${response.status}`);
    notice.value = `分享页主题 ${data.short} 已安装，摘要 ${String(data.digest).slice(0, 12)}…`;
    await load();
  });
}

/** 把控制台主题的令牌写到 :root 上；变量名与 style.css 完全一致。 */
async function applyConsoleTokens() {
  const style = document.getElementById("opsd-theme") as HTMLStyleElement | null;
  const target = style || document.createElement("style");
  if (!style) {
    target.id = "opsd-theme";
    document.head.appendChild(target);
  }
  if (w.demo) {
    target.textContent = "";
    return;
  }
  try {
    const active = await api("/themes/active");
    const tokens = active.console?.tokens;
    if (!tokens) {
      target.textContent = "";
      return;
    }
    const rules: string[] = [];
    const block = (selector: string, values: Record<string, string>) => {
      const body = Object.entries(values || {})
        .map(([key, value]) => `${key}: ${value};`)
        .join(" ");
      if (body) rules.push(`${selector} { ${body} }`);
    };
    block(":root", tokens.light);
    // 深色令牌写在 data-theme="dark" 上，与内置令牌的层叠方式一致
    block(':root[data-theme="dark"]', tokens.dark);
    target.textContent = rules.join("\n");
  } catch {
    target.textContent = "";
  }
}

function pickPackage(event: Event) {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  if (file) {
    packageName.value = `${file.name}（${Math.round(file.size / 1024)} KB）`;
    installSharePackage(file);
  }
}

onMounted(async () => {
  await load();
  await applyConsoleTokens();
});
</script>
<template>
  <section class="settings-section">
    <h2>主题</h2>
    <Notice v-if="error" tone="danger">{{ error }}</Notice>
    <Notice v-if="notice">{{ notice }}</Notice>
    <p class="muted">
      两条主题线的粒度不同：<strong>控制台主题是令牌级</strong>的，只覆盖颜色、圆角与密度，
      不替换组件结构——控制台里有终端、任务与表单，整体替换的风险不可接受。
      <strong>分享页主题是包级</strong>的，会替换整个分享页前端，因为它只是只读展示。
      同时覆盖两者的主题也是合法的，但两个选择器分别设置。
    </p>

    <Field label="控制台主题">
      <Select
        v-model="consoleActive"
        aria-label="控制台主题"
        :options="[
          { value: 'default', label: '内置主题（靛蓝）' },
          ...consoleThemes.map((t) => ({ value: t.short, label: label(t.name) })),
        ]"
      />
      <Button variant="primary" :disabled="busy" @click="activate('console')">应用</Button>
    </Field>
    <Field v-if="activeConsole?.fields?.length" label="主题设置">
      <template v-for="field in activeConsole.fields" :key="field.key">
        <Checkbox
          v-if="field.type === 'switch'"
          v-model="consoleSettings[field.key]"
          >{{ label(field.name) }}</Checkbox
        >
        <template v-else>
          <span class="muted">{{ label(field.name) }}</span>
          <Select
            v-if="field.type === 'select'"
            v-model="consoleSettings[field.key]"
            :aria-label="label(field.name)"
            :options="
              String(field.options || '')
                .split(',')
                .map((o: string) => ({ value: o.trim(), label: o.trim() }))
            "
          />
          <Input
            v-else
            v-model="consoleSettings[field.key]"
            :aria-label="label(field.name)"
          />
        </template>
      </template>
    </Field>

    <Field label="分享页主题">
      <Select
        v-model="shareActive"
        aria-label="分享页主题"
        :options="[
          { value: 'default', label: '内置分享页' },
          ...shareThemes.map((t) => ({ value: t.short, label: label(t.name) })),
        ]"
      />
      <Button variant="primary" :disabled="busy" @click="activate('share')">应用</Button>
    </Field>
    <Field v-if="activeShare?.fields?.length" label="主题设置">
      <template v-for="field in activeShare.fields" :key="field.key">
        <Checkbox v-if="field.type === 'switch'" v-model="shareSettings[field.key]">{{
          label(field.name)
        }}</Checkbox>
        <template v-else>
          <span class="muted">{{ label(field.name) }}</span>
          <Input v-model="shareSettings[field.key]" :aria-label="label(field.name)" />
        </template>
      </template>
    </Field>
    <Notice v-if="activeShare" tone="warning">
      分享页主题的设置是<strong>公开可读</strong>的，任何访客都能取到。
      不要把密钥、令牌或私密地址放进主题配置。
    </Notice>
  </section>

  <section class="settings-section">
    <h2>已安装主题</h2>
    <table v-if="themes.length" class="settings-table">
      <thead>
        <tr>
          <th>标识</th>
          <th>名称</th>
          <th>界面</th>
          <th>版本</th>
          <th>摘要</th>
          <th>操作</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="theme in themes" :key="theme.short">
          <td class="mono">{{ theme.short }}</td>
          <td>{{ label(theme.name) }}</td>
          <td>
            <Tag v-for="surface in theme.surfaces" :key="surface">{{
              surface === "share" ? "分享页" : "控制台"
            }}</Tag>
          </td>
          <td class="mono">{{ theme.version || "—" }}</td>
          <td class="mono">{{ theme.digest.slice(0, 12) }}…</td>
          <td>
            <Button variant="text" @click="removeTheme(theme.short)">删除</Button>
          </td>
        </tr>
      </tbody>
    </table>
    <p v-else class="muted">还没有安装任何主题，当前使用内置主题。</p>
    <p v-if="bothThemes.length" class="muted">
      其中 {{ bothThemes.map((t) => t.short).join("、") }} 同时声明支持两个界面，可在上面分别选用。
    </p>
  </section>

  <section class="settings-section">
    <h2>安装主题</h2>
    <p class="muted">
      仅支持本地上传，<strong>不接入在线市场、不自动远程拉取</strong>。
      安装时由主控自行计算 SHA-256 摘要并记录，包内声明不作数。
      分享页主题包必须是 zip，根目录包含 <code class="mono">theme.json</code> 与
      <code class="mono">index.html</code>；解压时拒绝越界路径与超大内容。
    </p>
    <Field label="控制台主题清单">
      <Input
        v-model="manifestText"
        aria-label="控制台主题清单 JSON"
        placeholder='{"short":"indigo-soft","name":"柔和靛蓝","surfaces":["console"],"tokens":{"light":{"--accent":"#0F2540"}}}'
      />
      <Button :disabled="busy || !manifestText" @click="installConsole">安装</Button>
    </Field>
    <Field label="分享页主题包">
      <input
        type="file"
        accept=".zip,application/zip"
        aria-label="分享页主题包"
        @change="pickPackage"
      />
      <span v-if="packageName" class="muted">{{ packageName }}</span>
    </Field>
  </section>
</template>
