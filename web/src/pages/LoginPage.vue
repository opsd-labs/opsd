<script setup lang="ts">
import { useWorkspace } from "../workspace";
import Button from "../components/ui/Button.vue";
import Input from "../components/ui/Input.vue";
import Notice from "../components/ui/Notice.vue";
import Field from "../components/resources/Field.vue";
const w = useWorkspace();
</script>
<template>
  <main class="login-page">
    <section class="login-panel">
      <h1>opsd</h1>
      <h2>管理员登录</h2>
      <Notice v-if="w.error" tone="danger">{{ w.error }}</Notice>
      <form @submit.prevent="w.login">
        <Field label="管理员密码"
          ><Input
            v-model="w.password"
            type="password"
            required
            autocomplete="current-password" /></Field
        ><Button
          type="submit"
          variant="primary"
          :disabled="w.busy"
          :busy="w.busy"
          >{{ w.busy ? "正在登录…" : "登录" }}</Button
        >
      </form>
      <Button variant="text" @click="w.startDemo">浏览演示 · 不执行操作</Button>
    </section>
  </main>
</template>
