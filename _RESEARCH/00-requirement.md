# PREPARE — 需求基线 (one-line requirement)

> 本文件是 lyco skill PREPARE 阶段产物。在任何代码/脚手架行动之前
> 必须先与用户确认这份需求基线。

## 一句话需求

**做一个统一框架，把 Minecraft 资源生成（skin / 3D / YSM）+ 音乐/歌词生成 + 代码生成
+ 通用图片生成 串成「多模态资源流水线」，让 lilyco-42 的 Galgame / Minecraft /
短视频工厂 都能消费。**

## 关键约束 (top 3)

1. **多模态统一** — 单一 CLI / MCP 入口，不同能力域（MC skin / 3D / music / code / image）
   可自由组合，不锁定单一用途
2. **生态对齐 lilyco 全家桶** — 能用 Rust 优先的（lilyco 框架本身的能力：
   「一个 struct 出 CLI/TUI/Web/MCP」），纯 Python 的（如混元 3D 的 SDK）也要兼容；
   输出物要能直接喂给 vp-pipeline（短视频）/ letsgal-ai（Galgame 资源）/ 已有 Minecraft 项目
3. **离线/本地优先** — 默认离线可用（模型本地推理），在线服务（混元 3D API、
   各类 LLM 网关）是可选 fallback；新 API 必须可一键切换

## 子需求 (待 PREPARE 确认)

| # | 能力域 | 输入 | 输出 | 备注 |
|---|--------|------|------|------|
| F1 | Minecraft skin 生成 | 文字描述 / 图片 / 参考角色 | PNG skin (64x64 双层) | SD 风格迁移 → skin 模板；**无 ≥10 星现成方案** |
| F2 | Minecraft 实体模型 (BSM/YSM) | 文字 / 图片 | .bsm/.ysm (BlockBench 格式) | YSM/BSM 是 BlockBench 格式，Y 轴旋转方向有差异 |
| F3 | GLB → BSM/YSM 转换 | .glb 3D 模型 | .bsm/.ysm | **无人之境**，GitHub 搜索 0 结果，必须自研 |
| F4 | 混元 3D 模型生成 | 文字/图片 | .glb/.obj | 本地开源（2.0 需 16GB）+ 腾讯云 API 双路 |
| F5 | 本地 3D 替代品 | 文字/图片 | .glb/.obj | TRELLIS.2（≥24GB）/ TripoSR（~6GB） |
| F6 | 音乐创作 | 文字 / 参考曲 | mp3 / wav（带人声或纯 BGM） | ACE-Step 首选（8GB，中文最佳；4090 上 1.74s/分钟） |
| F7 | 图片生成 | 文字 | PNG/jpg | ComfyUI 本地后端；**Rust 单二进制路线不存在** |
| F8 | 歌词生成 | 主题/情绪 | 纯文本 | LLM + prompt 模板（lilyco_chat / 在线） |
| F9 | 代码生成 | 需求 / 自然语言 | 源代码 | MCP server 给 lilyco_agent（或 CLI 双端） |
| F10 | （可选）视频生成 | 文字/图片 | mp4 | 走已有 vp-pipeline |

> **编号为全仓库唯一标准**，01/02/03/04 与本表一致（F1–F9 + 可选 F10）。

## 验收标准 (acceptance)

- 一条命令生成一个角色：「Minecraft skin+3D+专属 BGM+一张立绘」
- 生成的 skin 可直接进 MC 游戏；3D 资源进 BlockBench / Minecraft 加载
- 生成的 BGM 可直接进 vp-pipeline 剪视频
- 在线 API 与本地模型一键切换（同一 CLI flag）
- 全模块化原子构建（lilyco 信条 5）：每个能力域可单独跑通再组合

## 模糊点（需用户回答）

- Q1 (F2/F3): 3D 目标格式是 **Minecraft 实体模型 (BSM/YSM)** 还是 **通用 3D 资产**
  (glb/glTF)? YSM/BSM 仅 MC 用
- Q2 (F6): 音乐「带人声」还是**纯 BGM**? 人声本地极难、在线依赖 Suno 系
- Q3 (F4/F5): 混元 3D 必须走腾讯 API，还是也要支持 **本地 3D 生成模型**
  (Hunyuan3D-2 本地 / TRELLIS.2 / TripoSR)?
- Q4 (F9): 代码生成是 **CLI 工具**还是 **MCP server 给 lilyco_agent 用**?
- Q5: MVP 首个场景选哪个？（MC 三件套 / 歌词+BGM / skin 单点 / 代码 MCP）

## 决策快照（goal-round defaults, 2026-07-21）

用户未显式回答 Q1-Q5，按**默认值**推进 MVP，触发重审条件如下：

| 问题 | 默认 | 触发重审条件 |
|---|---|---|
| Q1 3D 主格式 | A: BSM/YSM 优先 | S2 验证 BSM 兼容失败 -> 切 B (glb) |
| Q2 音乐 | A: 纯 BGM 本地 | S3 ACE-Step 中文不达标 -> 切 B (带人声在线) |
| Q3 混元 | C: 本地 + API 并存 | 本地 S2 失败 + API 报价可接受 -> 切 A (API 主力) |
| Q4 代码生成 | C: CLI + MCP 双端 | S5 不需要 MCP -> 只出 CLI (B) |
| Q5 MVP 场景 | A: MC 三件套 | 用户改主意时重排 |

*默认值会在 S0-S5 验证过程中按上表触发条件自动调整。*