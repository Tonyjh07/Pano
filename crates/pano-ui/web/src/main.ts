import { getCurrentWindow } from "@tauri-apps/api/window";
import { mount } from "svelte";
import ManagerApp from "./manager/ManagerApp.svelte";
import ComponentWindow from "./components/ComponentWindow.svelte";
import "./app.css";

// 多窗口 SPA：每个 WebView 按自身 label 决定渲染内容
//（manager = 管理窗口；其余 = 对应组件窗口，1 组件 = 1 窗口）。
const label = getCurrentWindow().label;
const target = document.getElementById("app")!;

if (label === "manager") {
  mount(ManagerApp, { target });
} else {
  mount(ComponentWindow, { target });
}
