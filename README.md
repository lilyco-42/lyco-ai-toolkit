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

## 生成：Kaggle 免费 GPU

皮肤生成**真的能跑** —— 不需要本机有显卡：

```bash
lyco-cli skin generate --backend kaggle --owner <you> --slug lyco-skin-gen --seeds 7,42
```

它把 Rust 生成的笔记本推到 Kaggle，在免费的 Tesla T4 上跑 `AliceKJ/BLOCKv0.6`，
轮询到结束，拉回产物，**然后在本机**做 overlay 修复、结构校验和人形预览。

分工是刻意的：**Kaggle 只做它唯一不可替代的事（GPU 推理），UV 知识和修复逻辑归 Rust。**
生成的笔记本里**没有 UV 表**，CI 会断言这一点。

其余需要模型的部分（音频引擎、图像引擎、LLM）仍是 `--backend '<命令行>'` 子进程边界。

## F11 Spine：渲染，以及为什么它不随包发布

```bash
lyco-cli spine info model.atlas        # 页 / region / 是否声明 size、pma
lyco-cli spine normalize model.atlas   # 从 PNG 头补/修 size，幂等
lyco-cli spine version model.skel      # 读出需要的 Spine 版本
```

`lyco-spine-render` 能把 Spine 骨架摆姿势并**在 CPU 上**画成 PNG（`describe` /
`render --animation NAME`）。已用真实方舟数据验证：Spine 3.8.99、52 mesh、508 三角形。

**但它不随本仓库发布二进制**，原因很具体：

- 它链接官方 Spine 运行时，那是 **Spine Runtimes License Agreement（商业许可）**
- 本仓库源码是 MIT，**从它构建出的二进制不是** —— 发布需要你自己的 Spine 授权
- 所以默认 CI **只验证它能编译**，不产出 artifact；`windows-binary` 只发纯 MIT 的
  `lyco-cli.exe`

要拿到这个二进制，用仓库里的 **`build from source`** workflow（`workflow_dispatch`）：
你在自己的 fork 里触发，在你自己的授权下构建，产物只进你这次 run。

**注意：`pma` 是 Spine 4.x 的键。** 往 3.8 的 atlas 里写 `pma: true` 会让 3.8 运行时
**完全读不了那个文件** —— 实测确认，已在代码和 CLI 帮助里标注。

## 验证

每次 CI 跑：`test → clippy -D warnings → build → 6 组冒烟测试 → fmt`。
冒烟测试用外部实现交叉验证：Python 的 `wave` 解码我们写的 WAV、直接读 PNG 头核对尺寸、
现造 GLB 喂给转换、以及一次真实 MCP 会话。

## 已知缺口

- 真实 ACE-Step / ComfyUI / 混元 3D / LLM 接入需要 GPU 机器（皮肤走 Kaggle 已通）
- `keep-percent=12%` 这个启发式对某些 seed 会整体清空残留很少的 overlay 矩形，
  可能丢掉真实衣物细节；需要人眼判断是否调低
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
