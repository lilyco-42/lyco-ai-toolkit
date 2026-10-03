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
- [ ] **F3 几何转换**：GLB 三角面 → Bedrock 盒模型（体素化 / 盒拟合）
      —— 这是本项目最硬的算法部分，尚未开始
- [ ] **F2 humanoid**：标准玩家模型生成器（把 F1 皮肤的 UV 映射到骨骼）
      —— 待核实 Bedrock 正/背面命名与皮肤 front/back 的对应关系
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

## S4 — F7 图像 ✅ / F8 歌词 ⬜

### F7 图像 ✅ **离线部分已完成**
- [x] `lyco-image::render` — 纯函数程序化立绘（渐变 + 肩 + 头 + 发型 + 眼），
      由 prompt 的 FNV-1a 决定配色 ⇒ 确定性、可逐像素断言
- [x] `lyco-image::backend` — `ImageBackend` trait + `StubBackend` + `CommandBackend`
- [x] **验收**：产出被 `verify_image()` 解回，校验尺寸 **且拒绝纯色画布**
      —— 测试 16 passed
- [ ] 接 ComfyUI —— **需要 GPU 主机**（约 12GB 显存）

### F8 歌词 ⬜ 未开始
- [ ] `lyco-lyrics`：LLM 调用 + verse/chorus/bridge 结构约束
- [ ] **验收**：`lyco-cli lyrics --prompt '...'` 输出分段歌词

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
| F2/F3 模型 | lyco-bsm | 30 | humanoid 闭环 + Python 造真实 GLB 喂给 glb-info |
| F6 BGM | lyco-audio | 21 | **Python `wave` 独立解码**校验写出器 |
| F7 图像 | lyco-image | 16 | 纯色画布必须被拒绝 |
| S5 bundle | lyco-bundle | 6 | 四产物齐备 + 直读 PNG 头核对尺寸 |
| **合计** | | **108** | clippy -D warnings + fmt + build + 4 组冒烟测试 |

本地机器**无 NVIDIA GPU**（无 nvidia-smi / torch / ComfyUI），
因此所有 GPU 相关工作（F4/F6/F7）设计为子进程边界，本仓库只做可离线验证的部分。