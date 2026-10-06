import {
  computed,
  inject,
  onMounted,
  onUnmounted,
  reactive,
  ref,
  watch,
  type InjectionKey,
} from "vue";
import { api, apiPath, submit, setCsrf, type Entry, type Task, type EnrollmentSummary } from "./api";
import { demoNodes, demoTasks, demoPeers, demoMetrics } from "./demo";
/** 一级菜单。任务不再占用一级入口，见 AppShell 的顶栏任务面板。 */
export const navigation = [
  { id: "console", label: "控制台" },
  { id: "nodes", label: "节点" },
  { id: "database", label: "数据库" },
  { id: "docker", label: "容器" },
  { id: "firewall", label: "防火墙" },
  { id: "storage", label: "存储" },
  { id: "settings", label: "设置" },
];
/** 旧地址兼容：总览并入控制台，任务不再是一级页面。 */
const legacyPages: Record<string, string> = { overview: "console" };
export const taskNames: Record<string, string> = {
  inspect: "采集资源",
  docker_inspect: "容器详情",
  docker: "容器操作",
  pull_image: "拉取镜像",
  stack_import: "导入 Stack",
  stack_create: "创建 Stack",
  stack_plan: "配置变更计划",
  stack_apply: "部署 Stack",
  stack_control: "Stack 操作",
  firewall_plan: "防火墙计划",
  firewall_apply: "发布防火墙",
  peer_sync: "地址同步",
  host_inspect: "主机盘点",
  file_put: "写入文件",
  file_remove: "删除文件",
  file_rename: "重命名文件",
  file_chmod: "修改文件权限",
  file_chown: "修改文件属主",
  file_mkdir: "新建目录",
  db_inspect: "数据库只读巡检",
  storage_probe: "存储预检",
  storage_prepare: "存储磁盘准备",
  storage_deploy: "部署存储集群",
  storage_apply: "应用存储计划",
  storage_expand: "存储扩容",
};
export function createWorkspace() {
  const query = new URLSearchParams(location.search);
  const requested = legacyPages[query.get("page") || ""] || query.get("page") || "";
  const page = ref(
    navigation.some((n) => n.id === requested) ? requested : "console",
  );
  const environment = ref(query.get("node") || ""),
    nodes = ref<Entry[]>([]),
    enrollments = ref<EnrollmentSummary[]>([]),
    tasks = ref<Task[]>([]),
    peers = ref<any>(null),
    /** 各节点最新指标，键为节点 ID。缺失表示尚未上报。 */
    metrics = ref<Record<string, any>>({});
  const authenticated = ref(false),
    loading = ref(true),
    refreshing = ref(false),
    busy = ref(false),
    password = ref(""),
    error = ref(""),
    notice = ref(""),
    demo = ref(false);
  const modal = ref(""),
    selected = ref<any>(null),
    form = ref<any>({}),
    token = ref<any>(null),
    /** 顶栏任务面板。任务不再占用一级菜单，改由面板与各资源页内记录承载。 */
    taskPanel = ref(false),
    theme = ref(localStorage.getItem("opsd.theme") || "system");
  const sessions = ref<{ id: string; container: any; terminal: boolean }[]>([]),
    activeSession = ref(""),
    terminalVisible = ref(false);
  let events: EventSource | undefined,
    timer: ReturnType<typeof setInterval> | undefined,
    debounce: ReturnType<typeof setTimeout> | undefined,
    refreshPromise: Promise<void> | undefined;
  const entries = computed(() =>
    nodes.value.filter(
      (e) =>
        !e.node.revoked &&
        (!environment.value || e.node.id === environment.value),
    ),
  );
  const node = computed(() =>
    environment.value
      ? entries.value.find((e) => e.node.id === environment.value) || null
      : null,
  );
  const online = computed(
    () => entries.value.filter((n) => n.connected).length,
  );
  const containers = computed(() =>
    entries.value.flatMap((e) =>
      (e.node.inventory?.docker?.data?.containers || []).map((c: any) => ({
        ...c,
        uid: `${e.node.id}:${c.id}`,
        node_id: e.node.id,
        node_name: e.node.name,
        connected: e.connected,
      })),
    ),
  );
  const stacks = computed(() =>
    entries.value.flatMap((e) =>
      (e.node.inventory?.stacks || []).map((s: any) => ({
        ...s,
        uid: `${e.node.id}:${s.project}`,
        node_id: e.node.id,
        node_name: e.node.name,
        connected: e.connected,
      })),
    ),
  );
  const failures = computed(() =>
    tasks.value.filter((t) =>
      ["failed", "uncertain", "blocked", "rollback_pending"].includes(t.status),
    ),
  );
  const activeTasks = computed(() =>
    tasks.value.filter(
      (t) =>
        ![
          "succeeded",
          "failed",
          "uncertain",
          "blocked",
          "rolled_back",
          "cancelled",
        ].includes(t.status),
    ),
  );
  const firewall = computed(() => node.value?.node.inventory?.firewall);
  async function guarded(fn: () => Promise<void>) {
    busy.value = true;
    error.value = "";
    try {
      await fn();
      return true;
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
      return false;
    } finally {
      busy.value = false;
    }
  }
  async function load() {
    if (demo.value) return;
    if (refreshPromise) return refreshPromise;
    refreshing.value = true;
    refreshPromise = (async () => {
      const [n, e, t, p, m] = await Promise.all([
        api("/nodes"),
        api("/enrollment-tokens"),
        api("/tasks"),
        api("/peer-addresses"),
        // 指标是不可选数据：接口失败不应让整个工作台报错，页面按「未上报」展示。
        api("/metrics/overview").catch(() => ({ nodes: [] })),
      ]);
      nodes.value = n;
      enrollments.value = e;
      tasks.value = t;
      peers.value = p;
      metrics.value = Object.fromEntries(
        (m?.nodes || []).map((r: any) => [r.node_id, r]),
      );
    })().finally(() => {
      refreshing.value = false;
      refreshPromise = undefined;
    });
    return refreshPromise;
  }
  function disconnect() {
    events?.close();
    if (timer) clearInterval(timer);
    if (debounce) clearTimeout(debounce);
    events = undefined;
    timer = undefined;
    debounce = undefined;
  }
  function connectEvents() {
    disconnect();
    events = new EventSource(apiPath("/events"));
    events.onmessage = () => {
      if (debounce) return;
      debounce = setTimeout(() => {
        debounce = undefined;
        load().catch((e) => (error.value = e.message));
      }, 250);
    };
    events.onerror = () =>
      (notice.value = "实时连接中断，正在重连；当前为最后采集状态");
    events.onopen = () => (notice.value = "");
    timer = setInterval(
      () => load().catch((e) => (error.value = e.message)),
      15000,
    );
  }
  async function login() {
    await guarded(async () => {
      const s = await api("/auth/login", "POST", { password: password.value });
      setCsrf(s.csrf);
      password.value = "";
      authenticated.value = true;
      await load();
      connectEvents();
    });
  }
  async function logout() {
    await guarded(async () => {
      if (!demo.value) await api("/auth/logout", "POST");
      disconnect();
      sessions.value = [];
      location.href = location.pathname;
    });
  }
  async function dispatch(id: string, action: any) {
    return guarded(async () => {
      if (demo.value) {
        notice.value = "演示模式：未提交服务器操作";
        return;
      }
      const result = await submit(id, action);
      notice.value = `已提交任务 ${result.task_id.slice(0, 8)}，等待执行结果`;
      await load();
    });
  }
  async function revokeEnrollment(nodeId: string) {
    await guarded(async () => {
      if (demo.value) {
        notice.value = "演示模式：未提交服务器操作";
        return;
      }
      await api(`/enrollment-tokens/${nodeId}`, "DELETE");
      notice.value = "待接入令牌已撤销";
      await load();
    });
  }
  async function refresh() {
    if (demo.value) {
      notice.value = "演示数据：未请求服务器";
      return;
    }
    for (const e of entries.value.filter((e) => e.connected))
      await dispatch(e.node.id, { type: "inspect" });
  }
  function open(name: string, value?: any) {
    selected.value = value;
    error.value = "";
    token.value = null;
    form.value =
      name === "node"
        ? { name: "", public_addresses: "", overlay_address: "", ssh_port: 22, install_mode: "host" }
        : name === "stack"
          ? { project: "", directory: "", files: "compose.yml", env_files: "" }
          : name === "rule"
            ? {
                id: crypto.randomUUID(),
                family: "4",
                direction: "input",
                protocol: "tcp",
                source: "",
                destination: "",
                action: "accept",
                enabled: true,
                ...value,
                ports: value?.ports?.join(",") || "",
              }
            : name === "image"
              ? { reference: "" }
              : {};
    modal.value = name;
  }
  function close() {
    modal.value = "";
    error.value = "";
  }
  async function inspectContainer(c: any) {
    if (demo.value) {
      open("detail", c);
      return;
    }
    await guarded(async () => {
      const t = await submit(c.node_id, {
        type: "docker_inspect",
        container: c.id,
      });
      open("detail", {
        task_id: t.task_id,
        status: "正在读取容器详情与资源使用",
      });
      await load();
    });
  }
  watch(tasks, () => {
    if (modal.value !== "detail" || !selected.value?.task_id) return;
    const t = tasks.value.find((t) => t.id === selected.value.task_id);
    if (t?.result || t?.error)
      selected.value = t.result || { error: t.error, status: t.status };
  });
  async function save() {
    await guarded(async () => {
      if (demo.value) {
        notice.value = "演示模式：表单已校验，未提交服务器操作";
        return;
      }
      if (modal.value === "node") {
        token.value = await api("/enrollment-tokens", "POST", {
          ...form.value,
          ssh_port: Number(form.value.ssh_port),
          public_addresses: form.value.public_addresses
            .split(",")
            .map((s: string) => s.trim())
            .filter(Boolean),
          overlay_address: form.value.overlay_address || null,
          install_mode: form.value.install_mode || "host",
        });
        await load();
        return;
      }
      const target = selected.value?.node_id || node.value?.node.id;
      if (!target) throw new Error("请先选择节点");
      let action: any;
      if (modal.value === "stack")
        action = {
          type: "stack_import",
          ...form.value,
          files: form.value.files.split(",").map((s: string) => s.trim()),
          env_files: form.value.env_files
            .split(",")
            .map((s: string) => s.trim())
            .filter(Boolean),
        };
      if (modal.value === "create-stack")
        action = {
          type: "stack_create",
          project: form.value.project,
          content: form.value.content,
        };
      if (modal.value === "image")
        action = { type: "pull_image", reference: form.value.reference };
      if (modal.value === "rule")
        action = {
          type: "firewall_plan",
          operation: {
            type: "rule_put",
            rule: {
              id: form.value.id,
              family: Number(form.value.family),
              direction: form.value.direction,
              protocol: form.value.protocol,
              source: form.value.source || null,
              destination: form.value.destination || null,
              ports: form.value.ports.split(",").filter(Boolean).map(Number),
              action: form.value.action,
              enabled: form.value.enabled,
            },
          },
        };
      if (modal.value === "compose")
        action = {
          type: "stack_plan",
          project: selected.value.project,
          content: form.value.content,
        };
      if (!action) throw new Error("未知表单操作");
      const t = await submit(target, action);
      modal.value = "";
      notice.value = `已提交任务 ${t.task_id.slice(0, 8)}，等待执行结果`;
      await load();
    });
  }
  async function executePlan(t: Task) {
    const type =
      t.action.type === "firewall_plan" ? "firewall_apply" : "stack_apply";
    if (
      await dispatch(t.node_id, {
        type,
        plan_id: t.result.plan_id,
        ...(type === "firewall_apply"
          ? {
              witness:
                nodes.value.find((n) => n.connected && n.node.id !== t.node_id)
                  ?.node.id || null,
            }
          : {}),
      })
    )
      close();
  }
  function startStream(c: any, isTerminal: boolean) {
    const id = `${c.node_id}:${c.id}:${isTerminal}`;
    if (!sessions.value.some((s) => s.id === id))
      sessions.value.push({ id, container: c, terminal: isTerminal });
    activeSession.value = id;
    terminalVisible.value = true;
  }
  function endStream(id: string) {
    sessions.value = sessions.value.filter((s) => s.id !== id);
    activeSession.value = sessions.value.at(-1)?.id || "";
    if (!activeSession.value) terminalVisible.value = false;
  }
  function applyTheme() {
    document.documentElement.dataset.theme =
      theme.value === "system"
        ? matchMedia("(prefers-color-scheme: dark)").matches
          ? "dark"
          : "light"
        : theme.value;
    localStorage.setItem("opsd.theme", theme.value);
  }
  const media = matchMedia("(prefers-color-scheme: dark)");
  watch(theme, applyTheme);
  applyTheme();
  media.addEventListener("change", applyTheme);
  function startDemo() {
    disconnect();
    demo.value = true;
    authenticated.value = true;
    nodes.value = structuredClone(demoNodes);
    enrollments.value = [];
    tasks.value = structuredClone(demoTasks);
    peers.value = structuredClone(demoPeers);
    metrics.value = structuredClone(demoMetrics());
    loading.value = false;
  }
  watch([page, environment], () => {
    const url = new URL(location.href);
    url.searchParams.set("page", page.value);
    environment.value
      ? url.searchParams.set("node", environment.value)
      : url.searchParams.delete("node");
    history.replaceState(null, "", url);
  });
  onMounted(async () => {
    if (query.has("demo")) {
      startDemo();
      return;
    }
    try {
      const s = await api("/auth/me");
      setCsrf(s.csrf);
      authenticated.value = true;
      await load();
      connectEvents();
    } catch (e) {
      if (authenticated.value) error.value = String(e);
    } finally {
      loading.value = false;
    }
  });
  onUnmounted(() => {
    disconnect();
    media.removeEventListener("change", applyTheme);
  });
  return reactive({
    page,
    environment,
    nodes,
    enrollments,
    tasks,
    peers,
    metrics,
    authenticated,
    loading,
    refreshing,
    busy,
    password,
    error,
    notice,
    demo,
    modal,
    selected,
    form,
    token,
    taskPanel,
    theme,
    entries,
    node,
    online,
    containers,
    stacks,
    failures,
    activeTasks,
    firewall,
    sessions,
    activeSession,
    terminalVisible,
    guarded,
    load,
    login,
    logout,
    dispatch,
    revokeEnrollment,
    refresh,
    open,
    close,
    inspectContainer,
    save,
    executePlan,
    startStream,
    endStream,
    startDemo,
  });
}
export const workspaceKey: InjectionKey<ReturnType<typeof createWorkspace>> =
  Symbol("工作台");
export function useWorkspace() {
  const workspace = inject(workspaceKey);
  if (!workspace) throw new Error("缺少工作台上下文");
  return workspace;
}
