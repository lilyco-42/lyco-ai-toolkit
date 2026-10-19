# lyco-spine-viewer

Spine 3.8 小人播放器（桌面 GUI）：加载一个 atlas/skel 对，**WebGL 实时
预览**（PRTS wiki 同款架构：官方 spine-ts 3.8 runtime 浏览器端逐帧 GPU
渲染），导出走 `lyco-spine-render` 的 headless CPU 光栅化，像素确定性。

## 构建

本 crate 是**独立 workspace**，不在 lyco-ai-toolkit 的 headless CI 里构建
（GUI 栈 tao+wry 不该拖慢它）。构建完全由 GitHub Actions 远端完成，
本机不编译、不运行：

```bash
gh workflow run "Spine Viewer GUI build" --repo lilyco-42/lyco-ai-toolkit
```

成功后在 Actions 页面 Artifacts 下载 `lyco-spine-viewer-windows`。

## 预览架构

播放曾走「逐帧 still」IPC：每帧 Rust 光栅化 → PNG → base64 → 传输 →
JS 解码，41ms 帧预算跟不上，画面闪烁。现在预览由**前端内嵌的官方
spine-webgl 3.8 runtime**（`assets/vendor/spine-webgl-3.8.js`，构建时注入
页面）在 requestAnimationFrame 里直接绘制，纹理只上传一次，播放全程
零 IPC、零编码；Rust 只负责把 atlas/skel/png 的字节读给前端（`read`）
与确定性导出。

## 使用

1. 运行 `lyco-spine-viewer.exe`
2. 填入 atlas（`.atlas`）与 skeleton（`.skel` / `.json`）路径
   —— 素材来自明日方舟导出，都是 Spine 3.8；贴图须与 atlas 同目录
   —— 或直接把文件**拖进窗口任意位置**（含输入框）：支持
   atlas / skel / json / **png**（配对同名兄弟文件）或**整个素材文件夹**
   （递归找 atlas），识别后自动加载；不认识的文件会在状态栏提示
3. 从动画下拉选一个，播放 / 拖进度 / 调预览速度
4. 画面：滚轮缩放（以指针为中心）、按住左键拖拽平移、双击复位
5. 「导出」（Rust headless，与预览视图无关）：单帧 PNG、整段 GIF，
   或「全部动画 GIF」逐个导出（可取消）；输出目录留空则写当前目录

## 许可

本二进制经由 `lyco-spine-render` 链接官方 Spine runtime，且页面内嵌
官方 spine-ts 3.8 runtime，两者都处于 **Spine Runtimes License
Agreement**（商用许可）之下：评估运行没有问题，**对外分发需要持有你
自己的 Spine 许可**。本仓库不做任何发布。

## IPC 接口

页面消息为 `|` 分隔文本，首字段是请求 id，次字段是动词：

| verb | 参数 | 返回 |
| :--- | :--- | :--- |
| `load` | atlas, skel | 版本/hash/mesh/骨骼/动画列表（含时长） |
| `read` | path | 文件字节（base64），喂给前端 spine runtime |
| `still` | atlas, skel, anim, time, w, h, r,g,b,a | PNG data URL + 绘制边界 |
| `export-png` | atlas, skel, anim, time, w, h, r,g,b,a, out | 输出路径 + 边界 |
| `export-gif` | atlas, skel, anim, frames, fps, w, h, r,g,b,a, out | 输出路径 + 帧数 |

`anim` 为 `-` 表示 setup pose。