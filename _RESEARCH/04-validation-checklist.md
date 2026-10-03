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

## S3（第 5 天）F6 BGM 单点
- [ ] lyco-bundle::bgm()：subprocess acestep --prompt '...' --duration 30
- [ ] 测试 ACE-Step 中文 LoRA 识别 1 句中文
- [ ] **验收**：lyco-cli bgm --prompt '...' 输出 30 秒 wav + 转 mp3

## S4（第 6 天）F7 图像 + F8 歌词单点
- [ ] lyco-bundle::portrait()：ComfyUI SDXL 出 512x512 portrait
- [ ] lyco-lyrics::generate()：本地 LLM -> verse/chorus/bridge 结构
- [ ] **验收**：lyco-cli lyrics --prompt '...' 输出分段歌词 + 时间标注（无）

## S5（第 7 天）bundle 整合
- [ ] lyco-cli bundle --prompt '...' 串行调用 S1-S4 4 个原子能力
- [ ] **验收标准（00-requirement.md 的 #1 acceptance）：**
    1. 命令返回 0
    2. 输出目录：skin.png + model.bsm + bgm.mp3 + portrait.png
    3. skin PNG 是 64x64 且 mcmeta 校验通过
    4. BSM 在 BlockBench 导入成功
    5. BGM 时长 >= 30s
    6. portrait 分辨率 >= 512x512
- [ ] 输出 bundle manifest.json 列出所有文件 + 生成参数

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
| F1 MC skin | lyco-mc-skin | 35 | CLI 端到端闭环 + 垃圾输入必须失败 |
| F3 GLB 解析 | lyco-bsm::glb | 7 | — |
| F2 Bedrock geometry | lyco-bsm::bedrock | 10 | 官方 armor_stand 样本往返 |
| **合计** | | **54** | clippy -D warnings + fmt + build |

本地机器**无 NVIDIA GPU**（无 nvidia-smi / torch / ComfyUI），
因此所有 GPU 相关工作（F4/F6/F7）设计为子进程边界，本仓库只做可离线验证的部分。
