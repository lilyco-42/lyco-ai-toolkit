# 03-decision.md — 自研 vs 用现成 决策矩阵

> 基于 01-candidates.md 的 22 个已验证候选。
> 原则（lilyco 信条）：
>   3. 主流覆盖 ≥80% 需求 → 用现成 (fork / 调用)
>   5. 最小原子构建：一个能力一个 crate，可组合
>   4. 反 NIH：不造轮子

## 决策矩阵（按功能域）

| 域 | 自研 / 用现成 | 选择 | 工作量估计 | 风险 |
|---|---|---|---|---|
| F1 MC Skin 64x64 | **自研**（调用 ComfyUI 出图 + 自研 palette 量化） | 自研 (palette quantize + skin UV 校验) | 3-5 天 | palette 6-8 色需手调，可能效果差 |
| F2 BSM/YSM 实体模型 | **自研**（BlockBench 太重型，GPLv3 不便） | 自研 (JSON/BSM 序列化) | 5-7 天 | 格式细节多，需反复对 BlockBench 验证 |
| F3 GLB -> BSM 转换 | **自研**（无现成仓库） | 自研 (mcmeta 格式映射) | 5-7 天 | glTF cube 与 MC cube 不对齐，需手动映射 |
| F4 Hunyuan3D | **用现成**（双路并存） | 调用 (开源于 MIT + 腾讯 API) | 1-2 天 | GPU 24G，或 API 付费 |
| F5 本地 3D (TRELLIS-2/TripoSR) | **用现成** | 调用 (Python 子进程) | 1 天 | VRAM 紧张时 OOM |
| F6 音乐生成 | **用现成**（ACE-Step） | 调用 (Python 子进程或 HTTP) | 1 天 | 中文 BGM 质量稳定 |
| F7 图像生成 | **用现成**（ComfyUI） | 调用 (ComfyUI Python API) | 1-2 天 | 用户原 Rust 单二进制路线不存在，必须放弃 |
| F8 歌词生成 | **自研**（LLM call + prompt template） | 自研 (Rust + lilyco_chat) | 2-3 天 | prompt 调优 |
| F9 代码生成 | **自研**（MCP server） | 自研 (Rust + MCP SDK) | 2-3 天 | 与 lilyco_agent 集成 |
| **统一 CLI** | **自研**（lyco framework） | Rust CLI + MCP 双端 | 3 天 | lilyco framework 已有，需扩展 |
| **统一 MCP** | **自研**（lyco framework） | Rust MCP server | 2 天 | 同上 |

## 总体决策

**新建 repo：lyco-ai-toolkit（名字待定）**
- 形态：Rust workspace，4 个 crate + 1 个 MCP server
- 不重新造：
  - ComfyUI（图像生成后端，HTTP/Python 子进程）
  - ACE-Step（音乐生成，Python 子进程或 HTTP）
  - Hunyuan3D-2 / TRELLIS.2（3D 生成，Python 子进程 + 腾讯 API 双路）
- 自研（核心差异化）：
  1. **mc-skin-palette**：SD/ComfyUI 出图 → 64x64 → 6-8 色 palette 量化 → MC skin PNG（无现成）
  2. **glb-to-bsm**：glTF/glB → 解析/重映射 → BSM (BlockBench JSON) → 可选转 YSM（无现成）
  3. **mc-char-bundle**：组合 1 + Hunyuan3D + ACE-Step → 输出 'character bundle'（skin.png + model.bsm + bgm.mp3 + portrait.png），一次性 CLI 命令
  4. **lyco-lyrics**：LLM prompt 模板 → 中文歌词（lilyco_chat 调用）
  5. **lyco-codegen-mcp**：MCP server 给 lilyco_agent
- 不做：
  - 不造 BlockBench Rust 版（GPLv3 太重，且非核心）
  - 不造 Flux Rust 推理（不存在 Rust 项目，ComfyUI 替代）
  - 不造 stable-diffusion-rs（同上）
  - 不造 MuseGAN 替代品（ACE-Step 已覆盖）

## 用户决策待答（Q1-Q5）

- Q1 (主格式 BSM/YSM vs glb)：默认 BSM/YSM
- Q2 (音乐路线 BGM vs 人声)：默认 BGM 本地
- Q3 (混元 3D 主力 vs 备选)：默认 A/B 并存（本地 TRELLIS 为主，混元备）
- Q4 (代码生成 MCP only vs CLI+MCP)：默认 MCP 双端
- Q5 (MVP 选哪个)：默认 MC 角色三件套（最完整闭环）

## MVP Path（DEEP-DIVE 阶段）

1. **Day 1**：搭 Rust workspace + lyco framework 接入 + `cargo new` 4 个 crate 骨架
2. **Day 2**：mc-skin-palette 跑通 ComfyUI call → palette quantize → skin.p**NG**
3. **Day 3**：lyco-lyrics 跑通 lilyco_chat call → 输出 txt
4. **Day 4**：Hunyuan3D-2 跑通 image → glb (本地 GPU 或 API)
5. **Day 5**：glb-to-bsm 跑通 → 输出 .bsm
6. **Day 6**：mc-char-bundle 整合：CLI 一条命令输出 4 个文件
7. **Day 7**：MCP server 包一层给 lilyco_agent 用

## 风险与缓解
- GPU 显存：TRELLIS-2 / ACE-Step / ComfyUI 同时跑可能 OOM
  → 缓解：串行执行（同一时间一个子进程）
- 中文质量：ACE-Step 中文 BGM 还需 prompt 调优
  → 缓解：复用 Muzic 的 prompt 模板（[microsoft/muzic](https://github.com/microsoft/muzic)）
- glb-to-bsm 格式细节：mcmeta 文档不全
  → 缓解：fork 一份 BlockBench 测试样本，用 roundtrip 测试

---
*候选 URL 全部已 200/301/302 验证，无幻觉。*