# lyco-spine-viewer

Spine 3.8 小人播放器（桌面 GUI）：加载一个 atlas/skel 对，播放动画，
导出单帧 PNG 或整段 GIF。复用 `lyco-spine-render` 的 headless CPU 光栅化，
GUI 不新增渲染代码。

## 构建

本 crate 是**独立 workspace**，不在 lyco-ai-toolkit 的 headless CI 里构建
（GUI 栈 tao+wry 不该拖慢它）。构建完全由 GitHub Actions 远端完成，
本机不编译、不运行：

```bash
gh workflow run "Spine Viewer GUI build" --repo lilyco-42/lyco-ai-toolkit
```

成功后在 Actions 页面 Artifacts 下载 `lyco-spine-viewer-windows`。

## 使用

1. 运行 `lyco-spine-viewer.exe`
2. 填入 atlas（`.atlas`）与 skeleton（`.skel` / `.json`）路径
   —— 素材来自明日方舟导出，都是 Spine 3.8；贴图须与 atlas 同目录
3. 点「加载」，从动画下拉选一个，播放 / 拖进度 / 调 fps
4. 画面：滚轮缩放（以指针为中心）、按住左键拖拽平移、双击复位
5. 「导出」：单帧 PNG、整段 GIF，或「全部动画 GIF」逐个导出每个动画
   （可随时再点一次取消）；输出目录留空则写当前目录

## 许可

本二进制经由 `lyco-spine-render` 链接官方 Spine runtime，处于
**Spine Runtimes License Agreement**（商用许可）之下：评估运行没有问题，
**对外分发需要持有你自己的 Spine 许可**。本仓库不做任何发布。

## IPC 接口

页面消息为 `|` 分隔文本，首字段是请求 id，次字段是动词：

| verb | 参数 | 返回 |
| :--- | :--- | :--- |
| `load` | atlas, skel | 版本/hash/mesh/骨骼/动画列表（含时长） |
| `still` | atlas, skel, anim, time, w, h, r,g,b,a | PNG data URL + 绘制边界 |
| `export-png` | atlas, skel, anim, time, w, h, r,g,b,a, out | 输出路径 + 边界 |
| `export-gif` | atlas, skel, anim, frames, fps, w, h, r,g,b,a, out | 输出路径 + 帧数 |

`anim` 为 `-` 表示 setup pose。