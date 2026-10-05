# 04-validation-checklist.md - 最小化验证清单（MVP）

> 来源：03-decision.md 的 7 天 MVP 路径 + 02-niche-knowledge.md 的踩坑结论。
> 每条 TODO 都是**可独立检验**的最小验证，通过才推进到下一项。
> 信条 5（最小原子构建）：每个节点只验证一个能力域。

## S0（第 0 天）仓库骨架
- [x] cargo workspace：6 个 crate 占位（lyco-cli / lyco-mc-skin / lyco-bsm / lyco-bundle / lyco-lyrics / lyco-codegen-mcp）
- [x] gh repo create lilyco-42/lyco-ai-toolkit（private）
- [x] 写 _RESEARCH/00-04 共 5 份调研文档
- [x] .github/workflows/ci.yml：cargo check + fmt + clippy（fmt + clippy + build + CLI smoke）

## S1 — F1 MC Skin ✅ **已完成**（纯 Rust，离线可验证）

- [x] `lyco-mc-skin::format` — 64x64 box-UV 展开 + 12 部位/层锚点表
      （锚点实测自 `bs-community/skinview3d` `src/model.ts`，非记忆）
- [x] `lyco-mc-skin::palette` — median-cut 量化（确定性，无 RNG，默认 8 色）
- [x] `lyco-mc-skin::skin` — 载入/保存/模板 + 结构校验报告（含 legacy 64x32）
- [x] `lyco-cli skin {template,validate,info,quantize}` 接线完成
- [x] **验收证据**：CI 每次跑真实闭环 ——
      `template → validate → info(grep classic) → template --arm slim → info(grep slim)
       → quantize --colors 6 → validate --json → 喂随机字节必须非零退出`
- [x] 测试：**35 passed / 0 failed**
- [ ] 接入 ComfyUI subprocess（prompt → 图）—— **需要 GPU 主机**，本机无 GPU
      ⇒ 设计为子进程边界，`skin generate` 目前 fail-fast

## S2 — F2/F3 ⚠️ **目标格式已修正，进行中**

> ⚠️ 原计划写的是「BSM 单点」。二次核查证明 **BSM 这个格式不存在**，
> **YSM 是加密二进制**（详见 02-niche-knowledge.md 第五节）。
> 目标改为 **Bedrock geometry `.geo.json`**（开放、Blockbench 双向兼容）。

- [x] **F3 输入侧**：`lyco-bsm::glb` — GLB(binary glTF 2.0) 解析器
      容器/magic/版本、JSON+BIN 双 chunk、4 字节对齐、未知 chunk 跳过、
      VEC3 f32 位置 accessor、u8/u16/u32 索引 accessor、byteStride 交错数据
      —— 测试 **7 passed**
- [x] **F2 输出侧**：`lyco-bsm::bedrock` — Bedrock geometry 数据模型 + 写出 + 校验
      字段取自 Mojang 官方 `bedrock-samples` 真实文件
      （`resource_pack/models/entity/*.geo.json`），支持 box UV 与 per-face UV、
      无 cube 的关节骨、mirror、零尺寸面；校验覆盖 identifier 前缀、贴图尺寸、
      重名骨、缺失父骨、父子环、负尺寸、非有限值、未知面名
      —— 测试 **10 passed**（含对官方 armor_stand 样本的解析 + JSON 无损往返）
- [x] **F3 几何转换**：`lyco-bsm::voxel` + `lyco-bsm::convert`
      - `voxelize_surface()`：**按点到三角形距离**判定体素占用（半径 = 半个体素对角线）
      - `merge_boxes()`：三阶段贪心（先 X，再 Z，再 Y）把体素并成盒
      - `convert_glb()`：场景图 → 世界空间三角面 → 体素 → 盒 → Bedrock 几何
        **+ 生成贴图图集**（每个盒子一个格子，填该盒的平均源色）
      - 支持节点 TRS / matrix、线性色→sRGB、非三角图元跳过、场景图深度上限
      —— 测试 16 passed
      - **验收**：CI 用 python 现造 GLB 走完整转换（1 三角面 → 151 体素 → 15 盒），
        产出几何再次通过校验，贴图 PNG 头被独立读取
- [x] **F2 humanoid**：标准玩家模型生成器（6 骨 12 cube，UV 取自 F1 皮肤锚点表）
      —— Bedrock 的 box UV 与皮肤 box 展开同源，由官方 armor_stand 样本交叉验证
- [ ] 接 Hunyuan3D-2.0 subprocess（16GB）—— **需要 GPU 主机**
- **验收标准（修订）**：Blockbench 能导入产出的 `.geo.json`，骨骼 + per-face UV 完整

  **已获得的交叉验证**：官方 `armor_stand.geo.json` 的 `hat` 骨 cube `uv` 为 `[32, 0]`，
  正好等于 F1 核实过的皮肤**头部 overlay 锚点 (32,0)** ⇒ 两套坐标系同源。

## S3 — F6 BGM ✅ **离线部分已完成**（需要 GPU 的部分在边界外）

- [x] `lyco-audio::wav` — 手写 16-bit PCM 编**解**码器
      编码是为了产出，**解码是为了验证**：生成的音频必须能被独立解析出时长和峰值
      —— 测试 9 passed
- [x] `lyco-audio::backend` — `AudioBackend` trait + 两个实现
      - `StubBackend`：离线确定性合成（FNV-1a 由 prompt 决定音高），
        在 CLI 里以 `--backend stub` 显式暴露并打印警告，不可能被误认为真实音乐
      - `CommandBackend`：把生成器当子进程跑，只约定
        `--prompt <text> --duration <secs> --out <path>`，**不对任何引擎的 CLI 做假设**
      —— 测试 10 passed + 1 集成测试
- [x] **关键设计**：两条路径最后都过 `verify_wav()` —— 解码产出、校验时长与静音，
      **绝不把子进程的退出码当作产出成功的证据**
- [x] `lyco-cli bgm --prompt ... --duration N --out x.wav [--backend stub|'<命令行>']`
- [x] **验收证据**：CI 每次用 **Python 标准库 `wave` 独立解码**校验我们的 WAV 写出器 ——
      `2.0 2 2`（2 秒 / 立体声 / 16 位）；同一 prompt 两次产出字节相同；
      不存在的 backend 必须非零退出
- [ ] 接真实 ACE-Step —— **需要 GPU 主机**（8000 显存，4090 上 34x 实时）
- [ ] 转 mp3 —— 需要 ffmpeg/lame，尚未接入

## S4 — F7 图像 ✅ / F8 歌词 ✅

### F7 图像 ✅ **离线部分已完成**
- [x] `lyco-image::render` — 纯函数程序化立绘（渐变 + 肩 + 头 + 发型 + 眼），
      由 prompt 的 FNV-1a 决定配色 ⇒ 确定性、可逐像素断言
- [x] `lyco-image::backend` — `ImageBackend` trait + `StubBackend` + `CommandBackend`
- [x] **验收**：产出被 `verify_image()` 解回，校验尺寸 **且拒绝纯色画布**
      —— 测试 16 passed
- [x] `lyco-cli portrait` 单点命令
- [ ] 接 ComfyUI —— **需要 GPU 主机**（约 12GB 显存）

### F8 歌词 ✅ **已完成**
- [x] `lyco-lyrics::lyrics` — 文档模型 + 解析器 + **结构化校验器**
      采用 Suno/ACE-Step 通行的 `[Verse]` 标记惯例 ⇒ 产物可直接喂给 BGM 后端
      校验覆盖：无 section、空 section、缺 Chorus、未知标记、连续重复 section、
      超长行、行首尾空白
- [x] **未知标记原样保留** —— 解析失败的标记会以 `[Other x]` 形式写回，
      这是有损的；已修正为原样回写，并加回归测试钉死往返无损
- [x] `lyco-lyrics::backend` — `LyricsBackend` trait + `StubBackend` + `CommandBackend`
- [x] **验收**：产出的歌词被**重新解析并校验**；一个退出码 0 但只吐散文的写入器会被拒
      —— 测试 20 passed（含 1 集成测试）
- [ ] 接真实 LLM（本地 lilyco_chat / 在线网关）—— 配置一个 `--backend` 命令行即可

## S5 — bundle 整合 ✅ **已完成**

- [x] `lyco-bundle::build()` — 串起四个能力域，是唯一依赖全部四个 crate 的地方
      （依赖图因此是一棵树，不是一张网）
- [x] **每个产物写完都要验**：皮肤与几何重跑各自校验器；音频与图像解码回来核对尺寸/时长/非静音/非纯色。
      **四项全部成立才返回 bundle**
- [x] `lyco-cli bundle --prompt '...' --out DIR [--arm] [--bgm-duration] [--bgm-backend] [--image-backend]`
- [x] `manifest.json` 自描述：每个产物记录 kind / file / backend / bytes / **placeholder 标记**

### 验收标准对照（00-requirement.md）
| # | 要求 | 状态 |
|---|---|---|
| 1 | 命令返回 0 | ✅ |
| 2 | skin.png + model + bgm + portrait.png | ✅ （model 为 `.geo.json`，mp3 待 ffmpeg） |
| 3 | skin 是合法 64x64 | ✅ 就地重校验 + CI 直读 PNG 头核对 |
| 4 | 模型能被 MC 工具导入 | ⚠️ **未验证** —— 需人工在 Blockbench 里导入 |
| 5 | BGM 时长 >= 30s | ✅ 默认 30s，解码回读校验 |
| 6 | portrait >= 512x512 | ✅ 默认 512×512，CI 直读 PNG 头核对 |

### 诚实性设计
manifest 对 stub 产出的产物标 `placeholder: true`，并在 `notes` 里点名。
**一个 bundle 永远不会看起来像四次真实生成。**
CI 断言 `('model', False)` 与 `('portrait', True)` 同时成立。

## 跳过 / 延后到 v0.2+
- [ ] F9 MCP server（S5 验证通过之前不投入 - 过早抽象风险）
- [ ] 腾讯云 Hunyuan3D API 路线（本地 2.0 验证后再评估）
- [ ] TRELLIS.2 高保真路线（24GB 边界，S2 验收失败时才启用）

## 风险与对策（与 03-decision.md 同步）

| 风险 | 触发验证 | 对策 |
|---|---|---|
| ACE-Step 中文 LoRA 中文识别不精确 | S3 出 1 首人工验证 | 回退到 v1 基线，重训 LoRA |
| Hunyuan3D-2.0 16GB 在 24GB 卡 OOM | S2 跑 3 次取平均 | 切 --low_vram_mode 或回退 TRELLIS.2 |
| BSM serialize 与 BlockBench 不兼容 | S2 导入测试 | 解析 BlockBench src 找缺失字段 |
| ComfyUI + ACE-Step 同卡内存压力 | S5 串行 subprocess | 强制单进程（已规划）|
| 腾讯云 API 报价不可接受 | S2 后评估 | 全部走本地（2.0 + ComfyUI）|

— MVP 验证清单 —

## S0 验证证据（本次执行，已实测）

| 检查 | 命令 | 结果 |
|---|---|---|
| workspace 编译 | `cargo check --workspace` | 通过 |
| 构建 | `cargo build --workspace` | 通过 |
| 格式 | `cargo fmt --all -- --check` | 通过 |
| lint | `cargo clippy --workspace --all-targets` | 0 warning |
| CLI 版本 | `lyco-cli --version` | `lyco-cli 0.1.0-dev` |
| CLI 帮助 | `lyco-cli --help` | 6 个子命令可见 |
| fail-fast | `lyco-cli bundle --prompt x` | exit=1 + 指回本清单 |
| 远端 | `gh api repos/lilyco-42/lyco-ai-toolkit` | private, main |

## 证据汇总（自动生成，每次 CI 运行验证）

| 能力 | crate | 测试 | CI 额外验证 |
|---|---|---|---|
| F1 MC skin | lyco-mc-skin | 35 | skin 闭环 + 垃圾输入必须失败 |
| F2/F3 模型 + 转换 | lyco-bsm | 46 | humanoid + glb-info + **真实 GLB 转换** |
| F6 BGM | lyco-audio | 21 | **Python `wave` 独立解码**校验写出器 |
| F7 图像 | lyco-image | 16 | 纯色画布必须被拒绝 |
| F8 歌词 | lyco-lyrics | 20 | 标记结构断言 + 只吐散文的写入器必须被拒 |
| S5 bundle | lyco-bundle | 6 | 五产物齐备 + 直读 PNG 头核对尺寸 |
| F9 MCP server | lyco-mcp | 12 | **真实 stdio MCP 会话**（4 请求 / 4 回复） |
| F1 云端生成 | lyco-kaggle | 28 | 离线渲染校验 + **真实模型的产物做夹具回归** |
| **合计** | | **156** | clippy -D warnings + fmt + build + 6 组冒烟测试 |

本地机器**无 NVIDIA GPU**（无 nvidia-smi / torch / ComfyUI），
因此所有 GPU 相关工作（F4/F6/F7）设计为子进程边界，本仓库只做可离线验证的部分。
## CLI 面现状（2026-07-21）

**所有子命令都已实现，脚手架守卫（`fail_fast`）已删除。**

```
lyco-cli bundle    # skin + model + bgm + portrait + lyrics + manifest
lyco-cli skin      # template | validate | info | quantize | generate
lyco-cli bsm       # humanoid | validate | info | glb-info
lyco-cli bgm       # background music
lyco-cli lyrics    # sectioned lyric sheets
lyco-cli portrait  # a single portrait
```

需要模型的（扩散 / 音频引擎 / LLM）都通过 `--backend` 指向子进程；
内置 `stub` 后端在每个界面都被标注为 placeholder。
## 记录：一个被测试逼出来的算法缺陷（S2d）

第一版体素化用**重心坐标采样**（沿三角形边取固定步数）。结果：一个立方体壳被合并成 **286 个盒子**。

原因不是断言太严，而是采样步数被我硬编码上限为 24，于是**采样间距（0.118）大于体素边长（0.083）**，
壳上出现大量空洞。空洞会让贪心盒合并剧烈碎片化。

**修法**：改为**按点到三角形的距离**判定体素占用（阈值 = 半个体素对角线）。
这样无论三角形比体素大多少都不会有洞，代价是每个三角形只遍历它 AABB 覆盖的体素。
并补了两个测试钉死它：`the_shell_is_watertight_with_no_gaps` 与
`a_cube_shell_merges_into_a_handful_of_boxes`（后者在修复前会以 286 失败）。

教训：**硬编码的采样上限在几何算法里就是正确性漏洞。**
## F9 — MCP server ✅ **已完成**

> 原 F9 的定位是「代码生成 MCP」。实现时判断：**把整个 toolkit 暴露出去更有用** ——
> agent 想驱动这个项目是去**产出资产**，而这些能力每一个都已经是现成的 crate。
> 代码生成作为普通的 backend 位保留，而不是单独一个 server。crate 因此从
> `lyco-codegen-mcp` 更名为 `lyco-mcp`。

- [x] `lyco-mcp::jsonrpc` — 协议切片：success / error / tool_text / tool_error
- [x] `lyco-mcp::tools` — 12 个工具 + 各自的 JSON Schema
      `skin_template` `skin_validate` `skin_quantize` `model_humanoid` `model_validate`
      `glb_info` `model_from_glb` `lyrics_generate` `lyrics_validate` `bgm_generate`
      `portrait_generate` `bundle`
- [x] `lyco-mcp::server` — dispatch + stdio transport
- [x] `lyco-cli mcp` 启动 server
- **关键协议细节**：工具失败返回的是**成功的 JSON-RPC 响应 + `isError: true`**，
  不是协议级 error —— 这样模型能读到错误消息并换个做法。
- **通知不应答**：没有 `id` 的消息（如 `notifications/initialized`）按 spec 不回复，
  CI 断言 4 个请求只产生 4 条回复。
- **验收**：CI 里跑一次**真实 stdio 会话** —— initialize / 通知 / tools/list /
  调用 skin_template（真的产出 PNG 并再用 CLI 校验）/ 未知工具报 -32602。
  —— 测试 12 passed

```bash
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | lyco-cli mcp
```
## F1 云端生成 — Kaggle 免费 GPU ✅ **已跑通**

这是补上「只会造地基建不出房子」的那一步：**真的生成了东西。**

### 怎么跑

```bash
lyco-cli skin generate --backend kaggle --owner <you> --slug lyco-skin-gen --seeds 7,42
# 或分步
lyco-cli kaggle render --owner <you> --slug lyco-skin-gen --seeds 7,42 --out out/kernel
lyco-cli kaggle push   --dir out/kernel
lyco-cli kaggle status <you>/lyco-skin-gen
lyco-cli kaggle fetch  <you>/lyco-skin-gen --out out/kaggle-output
```

### 真实运行记录

| 项 | 值 |
|---|---|
| kernel | `lilyco4242/lyco-skin-generator`（v1） |
| GPU | Tesla T4 15.6 GB |
| 模型 | `AliceKJ/BLOCKv0.6`（Flux2Klein 4B，CPU offload，加载 127s） |
| 配置 | `float16` / `steps=30` / `guidance=4.0` / 参考图 resize 512 |
| seed 7 | 84.8s，12206 色，`ok` |
| seed 42 | 96.8s，12294 色，`ok` |
| 空图 | 0 |
| 结果 | `KernelWorkerStatus.COMPLETE` |

两张真实皮肤已作为夹具提交：`tests/fixtures/blockv06_seed{7,42}.png`。

### 白块问题：真实复现并量化

模型输出是 `mode=RGB`、`alpha_min=alpha_max=255` —— **根本没有 alpha 通道**。
于是它学到的白色背景**全部留在了第二层**：

| 部位 | seed 7 白像素 | seed 42 白像素 | 面积 |
|---|---|---|---|
| hat | 338 | 335 | 512 |
| jacket | 308 | 345 | 384 |
| rSleeve | 195 | 229 | 256 |
| lSleeve | 211 | 232 | 256 |
| rPant | 238 | 236 | 256 |
| lPant | 238 | 236 | 256 |
| **合计** | **1528 / 1920** | **1613 / 1920** | |

**80–84% 的外套层是白底。** 原样发布，角色在游戏里就是一坨白块 —— 修复前后的预览
对比在 CI 的 `lyco-repaired-skins` artifact 里。

修复结果：seed 7 清掉 **1564 px**（剩 2532），seed 42 清掉 **1743 px**（剩 2353）。

### 这一轮抓出的三个真 bug

**1. Kaggle 用 title 推 slug，不看 id。** 以 id `lyco-skin-gen` + title
"Lyco Skin Generator" 推送，Kaggle 归档成了 `lyco-skin-generator`，导致后续
`kernels status` 找不到。已加 `slugify()` 并在 `validate()` 里强制一致。

**2. 预览画不出它存在的理由。** `character_view` 原来只画 base 层，于是
**它无法显示白块** —— 而它存在的全部意义就是分辨好皮肤和白块。已改为 base + overlay
source-over 合成。这个 bug 只有在真实产物上才暴露。

**3. CI 断言照单个种子调参。** 我写了 `after >= 2400`，seed 7 过、seed 42 挂。
已改成断言真正的不变量（清理量在 1400–1920、算术自洽、角色仍在）。

### keep-percent 阈值的已知副作用

seed 42 有 **5 个 overlay 矩形**（jacket / 双袖 / 双腿）清理后残留低于 12%，被整体清空。
这个 12% 是沿用你可以工作的笔记本，但对本 seed 可能**丢掉了真实的衣物细节**。
是否调低需要人眼判断 —— 已记录，未擅自改动。
## F11 渲染：动画播放 ✅

`lyco-spine-render render <atlas> <skel> out.png --animation <name> --time <t>`

真实方舟阿米娅实测（14 个动画）：

```
Attack  Attack_Begin  Attack_End  Default  Die  Idle  Skill
Skill_2  Skill_2_Begin  Skill_2_End  Skill_Begin  Skill_End  Start  Stun
```

三个姿势渲染结果不同（Idle 站姿 / Attack 出拳 / Skill 蹲伏），说明动画采样生效。
**步进按 1/60 秒固定推进**，而不是一次跳到位 —— 动画在大 delta 和小 delta 下
采样结果不同，固定步长才可复现。

## 授权与分发安排 ✅

| 东西 | 许可 | 发布方式 |
|---|---|---|
| 源码（全部 crate） | MIT | GitHub |
| `lyco-cli.exe` | MIT | `ci` workflow artifact |
| `lyco-spine-render.exe` | **Spine 商业许可** | ❌ **不发布**；用 `build from source` workflow 自己编译 |

默认 CI **只验证 Spine 渲染器能编译**，不产出 artifact。要二进制就走
`.github/workflows/build-from-source.yml`（`workflow_dispatch`），在**你自己的
授权**下构建，产物只进你这次 run。工作流开头会打印许可提示。

已实测：`gh workflow run 'build from source' -f target=lyco-spine-render` → success。
