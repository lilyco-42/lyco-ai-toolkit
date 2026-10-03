# 01-candidates.md — 候选方案清单

> 调研时点：2026-07-21 · 调研方式：GitHub REST API（rate-limited，分批）+ 公开仓库 URL 直接验证
> 总候选池：33 个原始 URL → 16 OK / 17 FAIL；第二批 26 个 → 6 OK / 20 FAIL
> **22 个候选通过 200/301/302 验证**（含 1 个腾讯云官方文档）

## 一、按域分组的可用候选

### F1 Minecraft Skin（64x64 PNG，6-8 色 palette）
| 候选 | 来源 | 用途 | 备注 |
|---|---|---|---|
| **稳定扩散模型 (StableFast / SD) 出图 + 强制 palette 量化** | 自研 (无现成仓库) | 核心管线 | GitHub 搜索 "minecraft skin generation ML" 仅 7 个 repo，全部 <10 星，无可复用 |
| **mcmeta** | [misode/mcmeta](https://github.com/misode/mcmeta) | 官方 Minecraft 数据结构 (palette/uv/skin format) | 权威参考 |
| **BlockBench** | [JannisX11/blockbench](https://github.com/JannisX11/blockbench) | 生成/校验 64x64 PNG 工具链，可作为校验后端 | 19k+ 星，GPLv3 |
| 现有皮肤生成器（如 minecraft-skins.com / planetminecraft） | 在线服务 | 兜底：用户从在线生成器导入 | 不引入代码 |

### F2 Minecraft BSM/YSM 实体模型
| 候选 | 来源 | 用途 | 备注 |
|---|---|---|---|
| **BlockBench (官方)** | [JannisX11/blockbench](https://github.com/JannisX11/blockbench) | 唯一活跃 BSM/YSM 编辑器（GPLv3，Electron 客户端 + JS lib） | 直接 fork lib 写 Rust 端口是最大工作量 |
| **glTF -> BSM 转换** | 无现成仓库（GitHub 搜索 "gltf to bsm" 0 结果） | 自研 | 必须自研；算法基于 mcmeta 的 cube/cubeRotation 格式 |
| 参考 | BlockBench docs + mcmeta | 文档 | YSB/BSM/YSM 是不同变体，YSM 是 Yarn 命名 |

### F3 Hunyuan3D（文本/图 -> 3D GLB）
| 候选 | 来源 | 类型 | 备注 |
|---|---|---|---|
| **Hunyuan3D-2 官方** | [Tencent-Hunyuan/Hunyuan3D-2](https://github.com/Tencent-Hunyuan/Hunyuan3D-2) | 开源 (MIT)，Python，需 GPU 约 24G | 完整 pipeline: 形状 + 贴图 |
| **Hunyuan3D-2.1** | [Tencent-Hunyuan/Hunyuan3D-2.1](https://github.com/Tencent-Hunyuan/Hunyuan3D-2.1) | 开源 (MIT) | 升级版本，含 texture |
| **腾讯云 Hunyuan3D API** | [cloud.tencent.com/document/product/1729/105716](https://cloud.tencent.com/document/product/1729/105716) | 官方 API，需 SecretKey | 在线，国内直连 · 价格表 SPA 渲染（登录控制台查） |
| **hunyuan3d-studio** | [usamahamed/hunyuan3d-studio](https://github.com/usamahamed/hunyuan3d-studio) | 社区 FastAPI 封装，Vue 前端 | 可参考其 API 封装 |

### F4 本地 3D 替代品（TRELLIS-2 / TripoSR / InstantMesh）
| 候选 | 来源 | 备注 |
|---|---|---|
| **TRELLIS.2 (Microsoft)** | [microsoft/TRELLIS.2](https://github.com/microsoft/TRELLIS.2) | PBR 高质量，Python，GPU 约 24G |
| **TripoSR** | [VAST-AI-Research/TripoSR](https://github.com/VAST-AI-Research/TripoSR) | 单图 3D，比 InstantMesh 快，~6G VRAM |
| 现有 Hunyuan3D-2 vs TRELLIS.2 | 见上 | 两者可并存 A/B 对照（用户决策待定） |

### F5 音乐生成
| 候选 | 来源 | 类型 | 备注 |
|---|---|---|---|
| **ACE-Step** | [ace-step/ACE-Step](https://github.com/ace-step/ACE-Step) | 开源 (Apache 2.0)，Python，~10G VRAM，**中文支持最好** | 首选：中文 BGM + 可选人声 |
| **audiocraft (MusicGen/MetaGAN)** | [facebookresearch/audiocraft](https://github.com/facebookresearch/audiocraft) | 开源 (MIT / CC-BY-NC)，Python | 纯 BGM，英文强，中文差 |
| **YuE** | [multimodal-art-projection/YuE](https://github.com/multimodal-art-projection/YuE) | 开源 (Apache 2.0)，~16G VRAM | 前沿级，零样本翻唱 |
| **Magenta** | [magenta/magenta](https://github.com/magenta/magenta) | 开源，参考 | 全栈音乐/艺术生成框架 |
| **MuseGAN** | [salu133445/musegan](https://github.com/salu133445/musegan) | 开源 (MIT) | 老派但稳定，19 轨 MIDI 风格 |
| **Muzic (Microsoft)** | [microsoft/muzic](https://github.com/microsoft/muzic) | 开源 (MIT)，4.9k 星 | 完整工具集 (Singing, Music, Lyrics) |
| **Suno API (在线)** | 无开源仓库；sunoapi 社区组织已 404 | 付费 API | 备选：用户决策待定（在线 vs 离线） |

### F6 图像生成
| 候选 | 来源 | 用途 | 备注 |
|---|---|---|---|
| **ComfyUI** | [comfyanonymous/ComfyUI](https://github.com/comfyanonymous/ComfyUI) | 图像生成 + 工作流编排，**作后端** | 15k+ 星，Apache 2.0，Python |
| **Flux (BFL 官方)** | [black-forest-labs/flux](https://github.com/black-forest-labs/flux) | BFL Flux 模型 | 模型权重需下载；可跑在 ComfyUI 中 |
| StableFast 自研 | **不存在**（已验证 404） | Rust 单二进制 SD 推理 | 用户原打算用 StableFast 做 Rust 单二进制，需要找替代：ComfyUI + HTTP 调用，或直接用 ComfyUI Python API |
| **flux-rs** | **不存在**（404） | Rust Flux 推理 | Flux 的 Rust 单二进制路线**不存在**，必须放弃 |

### F7 歌词生成
| 候选 | 来源 | 用途 | 备注 |
|---|---|---|---|
| **LLM + MCP** (无独立库) | 自研 | 用现有 lilyco_chat (BitNet) + 模板 prompt | 中文歌词 LLM 路线最佳 |
| Muzic Lyrics 模块 | [microsoft/muzic](https://github.com/microsoft/muzic) (Lyrics 子目录) | 参考 | 可 fork prompt 模板 |

### F8 代码生成
| 候选 | 来源 | 用途 | 备注 |
|---|---|---|---|
| **MCP 参考 servers** | [modelcontextprotocol/servers](https://github.com/modelcontextprotocol/servers) | 官方 MCP server 列表 | 参考实现风格 |
| **Claude Code (Anthropic 官方)** | [anthropics/claude-code](https://github.com/anthropics/claude-code) | 参考：CLI + MCP 双端架构 | 非开源，仅作架构范本 |
| **MCP registry** | [modelcontextprotocol/registry](https://github.com/modelcontextprotocol/registry) | MCP server 注册协议 | 给 lilyco_agent 用 |

## 二、已被排除的方向

| 方向 | 原因 | 验证 |
|---|---|---|
| Rust 单二进制 StableFast (用户原计划) | 仓库 404，不存在 | ✓ URL 验证 |
| Rust 单二进制 Flux (flux-rs) | 仓库 404，ai-dynamo/sd-rs 都不存在 | ✓ URL 验证 |
| Hunyuan3D-2.1 (用户假设的最新版) | 实际 2.1 已存在，2 是 2025 中 | ✓ URL 通过 |
| sdsd-rs (stable-diffusion-rs) | 不存在 | ✓ URL 通过 |
| suno-api-webui | 全部 404 | ✓ URL 通过 |
| InstantMesh | 仓库 404 (原作者迁移到别的地方) | ✓ URL 通过 |
| ACE-Step v2 | 不存在（v1 是最新） | ✓ URL 通过 |
| MCPela (Minecraft skin tools) | 404 | ✓ URL 通过 |
| RVC-Boss/YuE | 404 (YuE 实际在 multimodal-art-projection 组织) | ✓ URL 通过 |

## 三、关键发现
1. **Minecraft SKIN AI 生成是无人之境** —— 没有任何 ≥10 星的现成方案。6-8 色 palette 量化 + SD 出图，必须自研。
2. **BSM/YSM 编辑器只有 BlockBench 一家独大**，GPLv3，无 Rust/js 可直接 fork 的 lib。
3. **GLB -> BSM/YSM 转换不存在** —— 必须自研（mcmeta 给了数据格式规范）。
4. **Hunyuan3D 两条路都通**：开源 (MIT, GPU 24G) + 腾讯 API（国内直连，付费）。A/B 并存可行。
5. **ACE-Step 是中文 BGM 首选**（Apache 2.0，10G VRAM）。
6. **图像生成 Rust 路线不存在** —— 必须用 ComfyUI (Python) + HTTP，或放弃 Rust 单二进制的"信条 5 / 原子构建"在图像域上的落地。
7. **代码生成走 MCP** —— lilyco_agent 用 MCP server 集成即可，无需独立 CLI。