<script setup lang="ts">
import { inject, ref } from "vue";
import { workspaceKey } from "../workspace";
import Button from "../components/ui/Button.vue";
import Input from "../components/ui/Input.vue";

const w = inject(workspaceKey)!;
const input = ref(w.password);
const submitting = ref(false);

async function login() {
  w.password = input.value;
  submitting.value = true;
  await w.login();
  submitting.value = false;
}
</script>

<template>
  <div class="login">
    <div class="login-inner">
      <div class="login-num">00</div>
      <h1 class="login-title">opsd</h1>
      <p class="login-sub">节点控制台</p>
      <form class="login-form" @submit.prevent="login">
        <label class="login-label" for="password">密码</label>
        <Input
          id="password"
          v-model="input"
          type="password"
          placeholder="输入访问密码"
          class="login-input"
          @keydown.enter="login"
        />
        <p v-if="w.error" class="login-error">{{ w.error }}</p>
        <Button class="login-btn" :disabled="submitting || !input" @click="login">
          {{ submitting ? "连接中…" : "进入" }}
        </Button>
      </form>
    </div>
  </div>
</template>

<style scoped>
.login {
  min-height: 100vh;
  display: flex;
  align-items: center;
  justify-content: flex-start;
  padding: 0 10vw;
  background: var(--bg);
}
.login-inner {
  max-width: 320px;
}
.login-num {
  font-size: 64px;
  font-weight: 700;
  line-height: 1;
  color: var(--line);
  margin-bottom: 16px;
  letter-spacing: -0.02em;
}
.login-title {
  font-size: 48px;
  font-weight: 700;
  line-height: 1.1;
  margin: 0 0 8px;
  color: var(--ink);
  letter-spacing: -0.02em;
}
.login-sub {
  font-size: 13px;
  color: var(--muted);
  margin: 0 0 40px;
}
.login-form {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.login-label {
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--muted);
}
.login-input {
  width: 100%;
}
.login-error {
  font-size: 12px;
  color: var(--accent);
  margin: 0;
}
.login-btn {
  align-self: flex-start;
  margin-top: 8px;
}
</style>
