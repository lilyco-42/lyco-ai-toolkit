# lyco-ai-toolkit

> 多模态 AI 资源流水线 - 一个 CLI/MCP 入口，覆盖：
> Minecraft skin (64x64) / BSM-YSM 3D 模型 / 音乐 (BGM) / 歌词 / 代码生成 / 通用图像
>
>
> Owner: lilyco-42 (云枢智创数码信息科技服务工作室)
>
> 状态：**scaffold**（DEEP-DIVE 阶段开始）- 调研见 _RESEARCH/
>
> 原则：lilyco 信条 3/4/5（反 NIH / 最小原子构建 / 一个能力一个 crate）

## Acceptance（一条命令一套资源）
```bash
lyco-cli bundle --prompt 'a quiet librarian, grey hair, soft light'
# => out/
#      skin.png       (64x64 slim model UV, mcmeta-validated)
#      model.bsm      (BSM/YSM, BlockBench-importable)
#      bgm.mp3        (ACE-Step, >= 30s)
#      portrait.png   (ComfyUI SDXL, >= 512x512)
#      manifest.json  (all paths + generation params)
```

## Layout
```
lyco-ai-toolkit/
+-- Cargo.toml              # workspace (resolver=2)
+-- crates/
|  +-- lyco-cli/            # 二进制：统一入口（clap subcommand）
|  +-- lyco-mc-skin/        # F1: 64x64 skin + palette quantize
|  +-- lyco-bsm/            # F2/F3: BSM/YSM serialize + GLB->BSM
|  +-- lyco-bundle/         # 编排：skin+3D+bgm+portrait
|  +-- lyco-lyrics/         # F8: lyrics 生成
|  +-- lyco-codegen-mcp/    # F9: MCP server (independent stdio)
+-- _RESEARCH/              # lyco skill preflight 报告
   +-- 00-requirement.md       # 一句话需求 + Q1-Q5 决策快照
   +-- 01-candidates.md        # 22 个已验证候选
   +-- 02-niche-knowledge.md   # 踩坑清单（VRAM/速度/命令）
   +-- 03-decision.md          # 自研 vs 用现成 矩阵（7 天 MVP）
   +-- 04-validation-checklist.md # S0-S5 最小化验证清单
+-- .github/workflows/      # CI（待补）
```

## 自研 / 用现成 摘要
| 域 | 选 | 量 |
|---|---|---|
| F1 MC Skin | 自研 palette + ComfyUI 出图 | 3-5 天 |
| F2 BSM | 自研 serialize（fork BlockBench MIT 参考）| 5-7 天 |
| F3 GLB->BSM | 自研（**无人之境**，404 已验证）| 含在 F2 |
| F4 本地 3D | Hunyuan3D-2.0 subprocess（16GB）| 0（已验证）|
| F5 高保真 3D | TRELLIS.2（24GB 边界，备选）| 0（备选）|
| F6 音乐 | ACE-Step subprocess（8GB，1.74s/60s）| 0（已验证）|
| F7 图像 | ComfyUI SDXL subprocess（12GB）| 0（已验证）|
| F8 歌词 | 自研 wrapper（LLM 调用）| 2 天 |
| F9 代码 MCP | 自研 stdio server（参考 modelcontextprotocol/servers）| 5 天（v0.2）|

## 24GB 显存拓扑（串行 subprocess）
| 模型 | 显存 | 用途 |
|---|---|---|
| ACE-Step | 8 GB | 音乐 |
| Hunyuan3D-2.0 | 16 GB | 3D 主力 |
| ComfyUI | 12 GB | 图像 |
| TRELLIS.2 | 24 GB | 高保真 3D（边界）|

## 快速开始（scaffold）
```bash
cargo build -p lyco-cli
./target/debug/lyco-cli --help
./target/debug/lyco-cli version
```

## 许可
MIT - 见 crates/*/LICENSE 与 workspace.package.license

*lyco skill preflight 完成时间：2026-07-21*