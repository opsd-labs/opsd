import { createApp } from "vue";
import "./style.css";
import "./share.css";
import ShareApp from "./ShareApp.vue";

// 分享页是独立入口：不加载工作台状态，也不注册任何管理侧路由。
createApp(ShareApp).mount("#share");
