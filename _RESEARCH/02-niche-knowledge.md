# 02-niche-knowledge.md — 踩坑清单与实战参数

> 来源：各开源库 README 直接验证（raw.githubusercontent.com）+ 腾讯云官方文档 + 已知社区共识
> 时点：2026-07-21

## 一、Hunyuan3D-2.1（Tencent · 本地 3D）

- VRAM 需求（官方 README 验证）:
  - 形状生成：10 GB
  - 纹理生成：21 GB
  - 形状 + 纹理：29 GB（24GB 卡不可跑）
- install 命令（已验证）:
  pip install torch==2.5.1 torchvision==0.20.1 torchaudio==2.5.1 --index-url https://download.pytorch.org/whl/cu124
  pip install -r requirements.txt
  pip install -e .
- CLI 入口: python3 gradio_app.py --low_vram_mode
- 踩坑:
  - Windows 官方无安装脚本，社区有 YanWenKun/Hunyuan3D-2-WinPortable（README 提及）
  - 29GB 上限对 24GB 卡不友好 → 用 --low_vram_mode 或改用 2.0 版本（2.0 总 16GB）
  - 模型权重在 Hugging Face tencent/Hunyuan3D-2.1（需接受 license）

## 二、Hunyuan3D-2.0（Tencent · 更省显存）

- VRAM（官方 README 验证）:
  - 形状：6 GB
  - 形状 + 纹理：16 GB（24GB 卡可跑）
- 模型子版本（--model_path 参数）:
  - tencent/Hunyuan3D-2mini — 小卡
  - tencent/Hunyuan3D-2mv — 中档
  - tencent/Hunyuan3D-2 — 默认
  - *-turbo 后缀 — FlashVDM 加速，推理时间减半
- 踩坑:
  - 需要 python3 setup.py install 两次（扩展编译）
  - Windows 同样依赖 WinPortable 社区工具

## 三、ACE-Step（音乐生成 · 中文歌词友好）

- VRAM（官方 README 验证）：最大 8 GB（消费级 3060/4060 可跑）
- 速度（官方 README 验证，RTF = 生成 1 分钟音乐所需时间）:
  - RTX 4090：34.48x RTF -> 1.74 秒
  - A100：27.27x RTF -> 2.20 秒
  - RTX 3090：12.76x RTF -> 4.70 秒
- CLI 入口（已验证）:
  acestep --torch_compile true --cpu_offload true --overlapped_decode true
- 中文支持：v1 内置中文歌词 LoRA + RapMachine（中文说唱 LoRA 开源）
- 踩坑:
  - 权重 ACE-Step/ACE-Step-v1-3.5B（3.5B 参数）
  - 无 Windows 原生二进制，需 WSL2 或 conda
  - ComfyUI 节点已有（ComfyUI_ACE-Step，2025.05.08 发布）-> 备选路径

## 四、TRELLIS.2（Microsoft · 图片转 3D）

- 参数规模（README 验证）：4B 参数，16x 空间下采样稀疏 voxel VAE
- VRAM（README 验证）：至少 24 GB（A100/H100 已验证）
- 分辨率：512^3 - 1536^3（高于 Hunyuan3D 的 1024^3）
- 依赖:
  - Python 3.8+，PyTorch 2.6.0 + CUDA 12.4
  - Conda 推荐（setup.sh 支持 --basic / --flash-attn）
  - flash-attn 后端，V100 等老卡需 xformers
- 踩坑:
  - 4B 参数 + flash-attn 编译是最大坑（首次约 30 分钟）
  - 输出格式：原生支持 glb / obj（**注意：glb 不等于 YSM，见第五节**）
  - 权重 microsoft/TRELLIS.2-4B 开源下载

## 五、BSM/YSM 转换 —— **已修正（2026-07-21 二次核查）**

> ⚠️ 本节早期版本称「YSM/BSM 是 BlockBench 格式」并引用「BlockBench 源码（MIT）」。
> **该结论是错的**，已作废。以下是实测事实。

### 5.1 Blockbench 到底支持哪些格式（实测 `js/formats/` 目录）

`JannisX11/blockbench`（6,021★，**GPL-3.0**，非 MIT）的实际格式模块：

| 目录 | 格式 |
|---|---|
| `bbmodel.js` | Blockbench 自有工程格式 `.bbmodel` |
| `bedrock/` | Bedrock 几何 / 动画 / attachable / voxel shape |
| `java/` | Java Block、**Modded Entity** |
| `optifine/` | OptiFine JEM / JPM |
| `standards/` | collada / fbx / **gltf** / obj / stl |
| `minecraft/skin.ts` | **Minecraft 皮肤**（F1 可参考） |

**结论：不存在 `bsm` 或 `ysm` 格式模块。** 这两个都不是 Blockbench 格式。

### 5.2 YSM 实测 = Yes Steve Model 模组，且模型是**加密二进制**

| 仓库 | 星 | 说明 |
|---|---|---|
| `YesSteveModel/YesSteveModel` | 43 | 上游模组本体 |
| `OpenYSM/YSMParser` | 589 | *"A Parser to recover Yes-Steve-Model **Encrypted Binaries**"* |
| `OpenYSM/OpenYSM` | 437 | OpenYSM 替代实现 |
| `Elaina69/Yes-Steve-Model-Repo` | 3,422 | 模型资源集合 |
| `Huli-fox/YesSteveModel-Unofficial` | 58 | 反移植到 1.7.10 |

- `.ysm` **不是** Model JSON，是模组私有格式；社区要写 parser 去「恢复加密二进制」
- 因此「GLB → YSM」不是格式转换，而是**逆向 + 重新编码**，风险与工作量远高于原估计

### 5.3 BSM 未找到任何标准格式

GitHub 搜索 `bsm minecraft model` / `bettermodel minecraft` 的结果里：
**没有任何格式叫 BSM**。唯一同名命中是一个 Bedrock 聊天过滤器（ML 模型，无关）。

最接近的可选项是 `toxicity188/BetterModel`（345★，*Modern Bedrock model engine for
Minecraft Java Edition*），它消费的是 **Bedrock 几何**，不是 BSM。

**⇒ BSM 需要用户澄清其确切含义后才能动工。**

### 5.4 修订后的路线

1. **确定的交换格式是 Bedrock geometry（`.geo.json`）** —— 开放、JSON、Blockbench
   可导入导出、ModelEngine / BetterModel 消费。**这是 F3 应当瞄准的落点。**
2. GLB → Bedrock geometry 需要自研（体素化 / 盒拟合），但格式是公开且可验证的。
3. YSM 单独作为一条高风险支线（需先评估 OpenYSM 的逆向成果与许可）。
4. BSM 待澄清。

## 六、Minecraft Skin 64x64（无人之境）

- 已有开源项目：misode/mcmeta（纯数据，含 skin 字段定义）
- AI 皮肤生成：GitHub 搜索 minecraft skin AI 无人做（调研结论）
- 路线：自研 mc-skin-palette（Rust, ~300 LOC），输入角色形象 prompt + palette hex，输出 64x64 PNG（slim / classic），依赖 image crate + ComfyUI subprocess

## 七、代码生成 MCP（F9 · lilyco_agent 接入）

- 参考：modelcontextprotocol/servers（参考实现）、modelcontextprotocol/registry（注册表）
- 踩坑：MCP server 必须独立 stdio 进程（不能嵌入 lyco-cli）
- 路线：lyco-codegen-mcp（Rust, ~500 LOC）暴露给 lilyco_agent / Claude Code / Cursor

## 八、统一运行拓扑（24GB 卡）

| 模型 | 显存 | 24GB? | 优先级 |
|------|------|-------|--------|
| ACE-Step | 8 GB | 可 | 音乐主力 |
| Hunyuan3D-2.0 | 16 GB | 可 | 3D 主力 |
| TRELLIS.2 | 24 GB | 边界 | 高保真备选 |
| ComfyUI (SDXL) | 12 GB | 可 | 图像主力 |
| Hunyuan3D-2.1 | 29 GB | 不可 | 跳过 |

结论：24GB 卡下 ACE-Step + Hunyuan3D-2.0 + ComfyUI 串行 subprocess 可行；TRELLIS.2 单跑，不与 Hunyuan 同时加载。

## 九、待验证项（标记 TODO 给 DEEP-DIVE）

- [ ] Tencent 云 Hunyuan3D API 实际报价（官方文档未公开单价）
- [ ] ComfyUI + ACE-Step 节点组合是否可并行（同卡 8+12=20GB 内存压力）
- [ ] glb-to-bsm 参考 sample（找 2-3 个 BlockBench 导出的 .bsm 样例文件结构）
- [ ] ACE-Step 中文歌词 LoRA 的中文 ASR 准确率
- [ ] TRELLIS.2 的 --resolution 1536 实际生成时间（vs 512）


## 十、腾讯云 Hunyuan3D API 调研结论（TODO 已部分验证）

- **文档 URL 验证（本次执行）：**
  - 105718 URL 实际指向云解析 DNS，**之前 01-candidates.md 中该条 URL 标注有误**（标题匹配命中 105716 路径）
  - 腾讯云搜索页 (cloud.tencent.com/search?q=hunyuan3d) 返回空结果
  - 腾讯云产品 code 1729 系列页均为 SPA 渲染，**无静态价格表**
- **结论：Hunyuan3D API 报价需登录腾讯云控制台「费用中心 → 产品定价」获取单价**，本仓库 DEEP-DIVE 阶段再做
- **推荐路径**：本地 Hunyuan3D-2.0 (16GB) 作主力，API 作弹性扩展，避免依赖单一计费源

*— 02 完成 —*
## 6. Spine 运行时：ROI 结论（2026-10-05 实测）

起因：想给 galgame 做 2D 骨骼角色，用方舟资源测试 Spine 渲染。

### 6.1 硬事实：方舟是 Spine 3.8，不是 4.3

从 `fexli/ArknightsResource` 拉真实文件 `char_002_amiya.skel`（179,880 字节），看头 35 字节：

```
idx  0 : 1c = 28      ← 长度字节（含终止符，实际 27 字节）
idx  1..27: 27 字节    ← hash "3zUmVpPXAB+DFpqv9VEzMu6Co8M"
idx 28 : 07 = 7       ← 长度字节（实际 6 字节）
idx 29..34: 6 字节     ← "3.8.99"
```

**长度字节包含终止符**（28 = 27+1，7 = 6+1）。这条格式细节是我第一次读错的。

### 6.2 候选全灭

| 候选 | 星 | 许可 | 能读 3.8? | 结论 |
|---|---|---|---|---|
| `Latias94/spine2d` | 17 | Apache-2.0 | ❌ **只支持 4.3 且硬拒绝其他** | 对你的用例不可用 |
| `jabuwu/rusty_spine` | 83 | **Spine Runtimes License** | ✅（转译自官方 C） | ❌ 商业许可，进不了 MIT |
| `axieinfinity/spine-rs` | 29 | MIT 外壳 + **spine-c FFI** | ✅ | ❌ 底层的 spine-c 是商业许可 |
| 官方 spine-c FFI | — | **Spine Runtimes License** | ✅ | ❌ 同上 |

`spine-rs` 的 MIT 只盖住它自己的胶水代码：`.gitmodules` 直接 submodule
`EsotericSoftware/spine-runtimes`，`spine-sys` 自称 "Bindings for spine-c library"。
**外层的 MIT 不改变底层运行时的许可。**

证据：`spine2d/src/export_version.rs` 里 `SPINE_RUNTIME_VERSION_PREFIX = "4.3"`，
配 `binary_rejects_non_43_spine_versions_like_cpp` —— 它是**主动拒绝**，不是解析失败。

### 6.3 结论

> **不存在许可宽松、且能读 Spine 3.8 的 Rust 运行时。**
> 所有能用的实现都源自官方运行时，而官方运行时是商业许可。

Spine 不是可以自由实现渲染器的开放格式。要正式用，须购买 Spine Editor 授权。

### 6.4 已落地（合法部分）

`lyco-spine` crate（MIT）+ `lyco-cli spine`：

- `spine info <atlas>` —— 页 / region / 是否声明 size、pma
- `spine normalize <atlas>` —— 从 PNG 头补/修 `size`、加 `pma`、**幂等**
- `spine version <skel>` —— **读出 .skel 需要的 Spine 版本**，判断运行时兼容性

真实数据验证（`char_002_amiya`）：

| 项 | 结果 |
|---|---|
| region 数 | 57（手工核对一致） |
| 原 atlas | **完全没有 `size` 字段** |
| 补入 | `size: 512,512`，Python 独立读 IHDR 确认 **512×512** |
| 副作用 | 后面 57 个 region **逐字节未变**；5595 → 5609 字节（+14） |
| 幂等 | 二次运行 `changed: false`，SHA256 相同 |
| 版本探测 | `3.8.99` / `spine 3.x`，正确 |

**资源不进仓库**：方舟美术属鹰角。真实文件只在本机 `.kaggle-scratch/` 用于验证，
CI 用合成的等价字节。
## 7. Spine 3.8 渲染：跑通了（2026-10-05 实测）

### 7.1 选型结果

| 候选 | 许可 | 能读 3.8 | 结论 |
|---|---|---|---|
| `Latias94/spine2d` | Apache-2.0 | ❌ 只 4.3，硬拒绝 | 不可用 |
| `jabuwu/rusty_spine` 0.8 | Spine 商业 | ❌ 4.2 | 版本不符 |
| **`jabuwu/rusty_spine3.8`** | Spine 商业 | ✅ | **采用** |
| `axieinfinity/spine-rs` | MIT 外壳 + spine-c FFI | ✅ | 底层仍是商业许可 |

`rusty_spine3.8` 是**纯 Rust**（无 `build.rs`、无 C 工具链），因为 C 运行时是
**作者时期转译**的而非构建时链接。所以能像 `lyco-cli` 一样在 CI 出二进制。
授权条款原文：*"The Rust code is licensed under dual MIT / Apache-2.0 ... you must
follow the Spine Runtimes License Agreement"*。

### 7.2 渲染管线（全部离线、纯 CPU）

```
lyco-spine-render describe <atlas> <skel>          # 版本 / mesh / 三角形数
lyco-spine-render render <atlas> <skel> out.png    # 摆姿势 + 光栅化 → PNG
```

- **姿态**：`rusty_spine` 的 `SkeletonController` 给出 `SkeletonRenderable`
  （`vertices: [[f32;2]]` + `uvs: [[f32;2]]` + `indices: [u16]` + 每件的
  `blend_mode` / `color` / `premultiplied_alpha`）
- **光栅化**：自己写的 CPU 光栅器（`lyco-spine-render::raster`），双线性采样、
  barycentric 重心坐标、四种混合模式。**不知道 Spine 的存在**，所以能离线单测
- **为什么不用 wgpu**：操作机没有真显卡，CI 完全没有。CPU 光栅器慢但可移植，
  而且**到处产生相同的字节**，这正是测试需要的

### 7.3 真实方舟数据实测

```
version:   3.8.99
hash:      3zUmVpPXAB+DFpqv9VEzMu6Co8M
meshes:    52
vertices:  976
triangles: 508
```

渲染结果：**完整角色**（阿米娅）。

### 7.4 两个把渲染卡死的 atlas 问题

**其一：缺 `size` → UV 除零。** 原版 `char_002_amiya.atlas` 没有任何 `size`
字段。C 运行时用 0×0 当页面尺寸算 UV → NaN → **什么都画不出来**（1909 字节的空图）。
**用 `lyco-spine` 补上 `size: 512,512` 后立刻渲染成功**（49287 字节）。

这直接印证了第 6 节：atlas 修复不是锦上添花，**它是渲染能工作的前提**。

**其二：`pma: true` 会让 3.8 运行时彻底读不了文件。** 实测：

| atlas 变体 | 3.8 C 运行时 |
|---|---|
| 原版（无 size） | ✅ 能读，但 UV 坏，画不出 |
| 只补 `size` | ✅ 能读，**渲染成功** |
| 补 `size` + `pma: true` | ❌ **`Failed to read file`** |

`pma` 是 **4.x 引入的键**。3.8 的解析器遇到不认识的页字段会**判定整页损坏**。
所以「补 pma」这条从 GPL 项目学来的经验，**对 3.8 是有害的** —— 已在代码和 CLI
帮助里标注。

### 7.5 资源不进仓库

方舟美术属鹰角。真实文件只在本机 `.kaggle-scratch/` 用于验证，仓库零字节。
