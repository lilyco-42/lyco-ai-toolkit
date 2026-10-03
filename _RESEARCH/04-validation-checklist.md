# 04-validation-checklist.md - 最小化验证清单（MVP）

> 来源：03-decision.md 的 7 天 MVP 路径 + 02-niche-knowledge.md 的踩坑结论。
> 每条 TODO 都是**可独立检验**的最小验证，通过才推进到下一项。
> 信条 5（最小原子构建）：每个节点只验证一个能力域。

## S0（第 0 天）仓库骨架
- [x] cargo workspace：6 个 crate 占位（lyco-cli / lyco-mc-skin / lyco-bsm / lyco-bundle / lyco-lyrics / lyco-codegen-mcp）
- [x] gh repo create lilyco-42/lyco-ai-toolkit（private）
- [x] 写 _RESEARCH/00-04 共 5 份调研文档
- [ ] .github/workflows/ci.yml：cargo check + fmt + clippy（占位，下次提交）

## S1（第 1-2 天）F1 MC Skin 单点
- [ ] lyco-mc-skin::palette_quantize()：6-8 色 k-means（imagequant crate）
- [ ] 接入 ComfyUI subprocess：固定 prompt -> 64x64 PNG
- [ ] 用 misode/mcmeta 的 UV schema 校验 slim 模型不重叠
- [ ] **验收**：lyco-cli skin --prompt '...' 输出 64x64 PNG + 校验通过

## S2（第 3-4 天）F2/F3 BSM 单点
- [ ] 收集 2-3 个 BlockBench 官方示例 .bsm（参考样本，不是闭源 repo 复制）
- [ ] 实现 lyco-bsm::serialize()：BSM JSON + 贴图输出
- [ ] 接 Hunyuan3D-2.0 subprocess（16GB @ 24GB 卡）：prompt -> GLB -> BSM
- [ ] **验收**：BlockBench 导入 BSM 不报错，骨骼 + 贴图完整

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