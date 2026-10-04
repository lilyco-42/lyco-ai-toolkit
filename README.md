# lyco-ai-toolkit

多模态 AI 资源流水线：一条命令产出 Minecraft 皮肤、Bedrock 模型、BGM、立绘、歌词。

> Owner: lilyco-42（云枢智创数码信息科技服务工作室）
> 调研与决策记录在 [`_RESEARCH/`](_RESEARCH/)

## 现在能做什么

```bash
lyco-cli bundle --prompt 'a quiet librarian' --out out
# => out/
#      skin.png        64x64 Minecraft 皮肤（合法，可校验）
#      model.geo.json  Bedrock 几何（Blockbench 可导入）
#      bgm.wav         背景音乐
#      portrait.png    立绘 512x512
#      lyrics.txt      分段歌词
#      manifest.json   每个产物的 backend 与 placeholder 标记
```

### 全部子命令

```
lyco-cli bundle    整包：skin + model + bgm + portrait + lyrics + manifest
lyco-cli skin      template | validate | info | quantize | generate
lyco-cli bsm       humanoid | validate | info | glb-info | from-glb
lyco-cli bgm       背景音乐
lyco-cli lyrics    分段歌词
lyco-cli portrait  单张立绘
lyco-cli mcp       在 stdin/stdout 上跑 MCP server，给 agent 用
```

## 接入 agent（MCP）

`lyco-cli mcp` 是一个 MCP server，用换行分隔的 JSON-RPC 2.0 收发。它把 12 个工具
暴露给 lilyco_agent / Claude Code / Cursor：`skin_template`、`skin_validate`、
`skin_quantize`、`model_humanoid`、`model_validate`、`glb_info`、`model_from_glb`、
`lyrics_generate`、`lyrics_validate`、`bgm_generate`、`portrait_generate`、`bundle`。

手动试一次：

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize"}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  | lyco-cli mcp
```

## 布局

```
lyco-ai-toolkit/
+-- crates/
|  +-- lyco-cli/        统一入口（clap）
|  +-- lyco-mc-skin/    F1  64x64 box-UV、调色板量化、皮肤校验
|  +-- lyco-bsm/        F2/F3 Bedrock 几何、玩家模型、GLB 解析、体素化转换
|  +-- lyco-audio/      F6  WAV 编解码、生成后端（stub + 子进程）
|  +-- lyco-image/      F7  程序化立绘、生成后端
|  +-- lyco-lyrics/     F8  歌词解析与结构校验、生成后端
|  +-- lyco-bundle/     S5  编排 + 自描述 manifest
|  +-- lyco-mcp/        F9  MCP server（JSON-RPC over stdio）
+-- _RESEARCH/          lyco skill 预研报告（00-04）
+-- scripts/            测试夹具生成器（造 GLB）
```

## 三个设计原则

1. **解码大于编码。** 音频、图像、歌词写完都要解回来核对；**绝不把子进程退出码
   当作产出成功的证据**。
2. **placeholder 必须显眼。** 内置 `stub` 后端在 CLI、manifest 的 `placeholder`
   字段和 `notes` 里三处标注，一个 bundle 永远不会看起来像五次真实生成。
3. **需要模型的藏在子进程后面。** 不对 ACE-Step / ComfyUI / 任何 LLM 的 CLI 做假设，
   只约定参数形状，用 `--backend` 指过去。

## 离线 / 在线

本仓库全部可离线构建与测试（156 个测试）。需要 GPU 的部分（扩散、音频引擎、LLM）
通过 `--backend '<命令行>'` 接到有显卡的机器上。开发机没有 GPU，所以这些路径目前
只能以 stub 跑通。

## 验证

每次 CI 跑：`test → clippy -D warnings → build → 6 组冒烟测试 → fmt`。
冒烟测试用外部实现交叉验证：Python 的 `wave` 解码我们写的 WAV、直接读 PNG 头核对尺寸、
现造 GLB 喂给转换、以及一次真实 MCP 会话。

## 已知缺口

- 真实 ACE-Step / ComfyUI / 混元 3D / LLM 接入需要 GPU 机器
- BGM 目前是 WAV；mp3 转码需要 ffmpeg
- 转换出的模型贴图是「每盒平均色」，不是对原网格纹理的 UV 重映射
- 模型能否被 Blockbench 导入**未经人工验证**（自动化只能验到 schema 层）

## 术语更正

调研初稿曾写「YSM/BSM 是 BlockBench 格式」——**错的**，已在
[`_RESEARCH/02-niche-knowledge.md`](_RESEARCH/02-niche-knowledge.md) 第五节更正：
Blockbench 没有这两个格式且是 GPL-3.0；YSM 是 Yes Steve Model 模组的加密二进制；
BSM 不存在。本项目落点是开放的 **Bedrock geometry**。

## 许可

MIT
