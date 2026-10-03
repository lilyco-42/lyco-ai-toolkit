# 01-candidates.md — 候选方案清单

> 调研时点：2026-07-21 · 调研方式：GitHub REST API（rate-limited，分批）+ 公开仓库 URL 直接验证
> 总候选池：33 个原始 URL → 16 OK / 17 FAIL；第二批 26 个 → 6 OK / 20 FAIL
> **22 个候选通过 200/301/302 验证**（含 1 个腾讯云官方文档）
>
> **能力域编号（全仓库唯一标准，与 00/02/03/04 一致）：**
> F1 MC skin · F2 BSM/YSM · F3 GLB→BSM · F4 混元3D · F5 本地3D · F6 音乐 · F7 图像 · F8 歌词 · F9 代码

## 一、按域分组的可用候选

### F1 Minecraft Skin（64x64 PNG，6-8 色 palette）
| 候选 | 来源 | 用途 | 备注 |
|---|---|---|---|
| **SD 出图 + 强制 palette 量化** | 自研（无现成仓库） | 核心管线 | GitHub 搜索 "minecraft skin generation ML" 仅 7 个 repo，全部 <10 星，无可复用 |
| **mcmeta** | [misode/mcmeta](https://github.com/misode/mcmeta) | 官方 MC 数据结构（palette/uv/skin format） | 权威参考 |
| **BlockBench** | [JannisX11/blockbench](https://github.com/JannisX11/blockbench) | 生成/校验 64x64 PNG 工具链 | 19k+ 星，GPLv3 |
| 在线皮肤生成器（minecraft-skins.com 等） | 在线服务 | 兜底：用户手动导入 | 不引入代码 |

### F2 Minecraft 实体模型（**目标格式待澄清**）

> ⚠️ 早期版本本节称「YSM/BSM 是 BlockBench 格式」——**错误，已作废**。
> 详见 [02-niche-knowledge.md](02-niche-knowledge.md) 第五节。

| 候选 | 来源 | 用途 | 备注 |
|---|---|---|---|
| ~~BSM~~ | **无此标准格式** | — | GitHub 搜索无任何名为 BSM 的模型格式 |
| **YSM** | [YesSteveModel/YesSteveModel](https://github.com/YesSteveModel/YesSteveModel) | Yes Steve Model 模组私有格式 | **加密二进制**，非 JSON；社区在逆向 |
| YSM 逆向成果 | [OpenYSM/YSMParser](https://github.com/OpenYSM/YSMParser) (589★) | 解密 parser | 需先评估其许可与完成度 |
| **Bedrock geometry** | [JannisX11/blockbench](https://github.com/JannisX11/blockbench) `js/formats/bedrock/` | **推荐的开放落点** | 开放 JSON，Blockbench 可导入导出 |
| BetterModel | [toxicity188/BetterModel](https://github.com/toxicity188/BetterModel) (345★) | Java 端渲染 Bedrock 模型 | 消费 Bedrock geometry，非 BSM |

### F3 GLB → Minecraft 模型转换
| 候选 | 来源 | 用途 | 备注 |
|---|---|---|---|
| **无现成方案** | GitHub 搜索 `gltf to bsm` **0 结果** | **必须自研** | 需要体素化 / 盒拟合，非纯格式转换 |
| glTF / GLB 规范 | Khronos glTF 2.0（开放标准） | 输入侧格式 | 容器 + JSON + BIN 三块，可离线解析 |
| mcmeta | [misode/mcmeta](https://github.com/misode/mcmeta) | 参考数据格式 | Blockbench 的 `js/formats/` 是更直接的格式权威 |

### F4 混元 3D（Hunyuan3D：文本/图 → 3D GLB）
| 候选 | 来源 | 类型 | 备注 |
|---|---|---|---|
| **Hunyuan3D-2 官方** | [Tencent-Hunyuan/Hunyuan3D-2](https://github.com/Tencent-Hunyuan/Hunyuan3D-2) | 开源（MIT），Python | 形状 6GB / 形状+纹理 16GB VRAM（官方 README 实测） |
| **Hunyuan3D-2.1** | [Tencent-Hunyuan/Hunyuan3D-2.1](https://github.com/Tencent-Hunyuan/Hunyuan3D-2.1) | 开源（MIT） | 形状 10GB / 纹理 21GB / 合计 **29GB** — 24GB 卡不可跑 |
| **腾讯云 Hunyuan3D API** | [cloud.tencent.com/document/product/1729/105716](https://cloud.tencent.com/document/product/1729/105716) | 官方 API，需 SecretKey | 国内直连；价格表 SPA 渲染，需登录控制台查 |
| **hunyuan3d-studio** | [usamahamed/hunyuan3d-studio](https://github.com/usamahamed/hunyuan3d-studio) | 社区 FastAPI 封装，Vue 前端 | 可参考其 API 封装方式 |

### F5 本地 3D 替代品（TRELLIS-2 / TripoSR）
| 候选 | 来源 | 备注 |
|---|---|---|
| **TRELLIS.2（Microsoft）** | [microsoft/TRELLIS.2](https://github.com/microsoft/TRELLIS.2) | 4B 参数，PBR 高质量，Python，**≥24GB VRAM**，输出 glb/obj |
| **TripoSR** | [VAST-AI-Research/TripoSR](https://github.com/VAST-AI-Research/TripoSR) | 单图 3D，快，约 6GB VRAM |
| Hunyuan3D-2 vs TRELLIS.2 | 见上 | 两者可并存 A/B 对照（用户决策待定） |

### F6 音乐生成
| 候选 | 来源 | 类型 | 备注 |
|---|---|---|---|
| **ACE-Step** | [ace-step/ACE-Step](https://github.com/ace-step/ACE-Step) | 开源（Apache 2.0），Python | **首选**：中文最佳，**最大 8GB VRAM**，4090 上 34.48x RTF |
| **audiocraft（MusicGen）** | [facebookresearch/audiocraft](https://github.com/facebookresearch/audiocraft) | 开源（MIT / CC-BY-NC），Python | 纯 BGM |
| **YuE** | [multimodal-art-projection/YuE](https://github.com/multimodal-art-projection/YuE) | 开源（Apache 2.0） | 约 16GB VRAM，前沿级零样本翻唱 |
| **Muzic（Microsoft）** | [microsoft/muzic](https://github.com/microsoft/muzic) | 开源（MIT） | 完整工具集（Singing / Music / Lyrics） |
| **Magenta** | [magenta/magenta](https://github.com/magenta/magenta) | 开源 | 全栈音乐/艺术生成框架 |
| **MuseGAN** | [salu133445/musegan](https://github.com/salu133445/musegan) | 开源（MIT） | 老派但稳定，19 轨 MIDI |
| Suno API（在线） | 无开源仓库（sunoapi 组织已 404） | 付费 API | 备选：在线 vs 离线待定 |

### F7 图像生成
| 候选 | 来源 | 用途 | 备注 |
|---|---|---|---|
| **ComfyUI** | [comfyanonymous/ComfyUI](https://github.com/comfyanonymous/ComfyUI) | 图像生成 + 工作流编排，**作后端** | 15k+ 星，Apache 2.0，Python |
| **Flux（BFL 官方）** | [black-forest-labs/flux](https://github.com/black-forest-labs/flux) | 模型权重 | 可跑在 ComfyUI 中 |
| Rust 单二进制 SD（StableFast） | **不存在**（404 已验证） | — | Rust SD 推理路线不存在，必须走 ComfyUI + HTTP |
| flux-rs | **不存在**（404 已验证） | — | Rust Flux 推理路线不存在，已放弃 |

### F8 歌词生成
| 候选 | 来源 | 用途 | 备注 |
|---|---|---|---|
| **LLM + 模板 prompt** | 自研（无独立库） | 用 lilyco_chat（BitNet）或在线 LLM | 中文歌词 LLM 路线最佳 |
| Muzic Lyrics 模块 | [microsoft/muzic](https://github.com/microsoft/muzic) | 参考 | 可 fork prompt 模板 |

### F9 代码生成
| 候选 | 来源 | 用途 | 备注 |
|---|---|---|---|
| **MCP 参考 servers** | [modelcontextprotocol/servers](https://github.com/modelcontextprotocol/servers) | 官方 MCP server 列表 | 参考实现风格 |
| **Claude Code（Anthropic 官方）** | [anthropics/claude-code](https://github.com/anthropics/claude-code) | 参考：CLI + MCP 双端架构 | 非开源，仅作架构范本 |
| **MCP registry** | [modelcontextprotocol/registry](https://github.com/modelcontextprotocol/registry) | MCP server 注册协议 | 给 lilyco_agent 用 |

## 二、已被排除的方向

| 方向 | 原因 | 验证 |
|---|---|---|
| Rust 单二进制 StableFast | 仓库 404，不存在 | ✓ URL 验证 |
| Rust 单二进制 Flux（flux-rs） | 404；ai-dynamo/sd-rs 均不存在 | ✓ URL 验证 |
| stable-diffusion-rs（sd-rs / sobercik） | 404 | ✓ URL 验证 |
| suno-api-webui | 全部 404 | ✓ URL 验证 |
| InstantMesh | 404（原作者已迁移） | ✓ URL 验证 |
| ACE-Step v2 | 不存在（v1 是最新） | ✓ URL 验证 |
| MCPela（Minecraft skin tools） | 404 | ✓ URL 验证 |
| bsm-tools（BlockBenchDev） | 404 | ✓ URL 验证 |
| RVC-Boss/YuE | 404（实际在 multimodal-art-projection 组织） | ✓ URL 验证 |

## 三、关键发现

1. **Minecraft SKIN AI 生成是无人之境** —— 没有 ≥10 星的现成方案；6-8 色 palette 量化 + SD 出图必须自研（F1）。
2. **BSM/YSM 编辑器只有 BlockBench 一家独大**（GPLv3），无可直接 fork 的 Rust/JS lib（F2）。
3. **GLB → BSM/YSM 转换不存在** —— 必须自研，mcmeta 提供数据格式规范（F3）。
4. **Hunyuan3D 两条路都通**：开源（MIT，2.0 需 16GB）+ 腾讯 API（国内直连，付费）。A/B 并存可行（F4）。
5. **ACE-Step 是中文 BGM 首选**（Apache 2.0，8GB VRAM，4090 上 1.74s/分钟）（F6）。
6. **图像生成 Rust 路线不存在** —— 必须用 ComfyUI（Python）+ HTTP（F7）。
7. **代码生成走 MCP** —— lilyco_agent 用 MCP server 集成即可（F9）。

*— lyco skill ACQUIRE 阶段完成 —*
