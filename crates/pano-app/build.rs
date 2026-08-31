// 生成 Tauri 所需资源（窗口图标 / 前端嵌入等，读取 tauri.conf.json）。
fn main() {
    tauri_build::build()
}
