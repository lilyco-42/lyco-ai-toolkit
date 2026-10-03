# 02-niche-knowledge.md — 踩坑清单与实战参数

> 来源：各开源库 README 直接验证（raw.githubusercontent.com）+ 腾讯云官方文档 + 已知社区共识
> 时点：2026-07-21

## 一、Hunyuan3D-2.1（Tencent · 本地 3D）

- VRAM 需求（官方 README 验证）:
  - 形状生成：10 GB
  - 纹理生成：21 GB
  - 形状 + 纹理：29 GB（24GB 卡不可跑）
- install 命令（已验证）:
  pip install torch==2.5.1 torchvision==0.20.1 torchaudio==2.5.1 --index-url https://download.pytorch.org/whl/cu124
  pip install -r requirements.txt
  pip install -e .
- CLI 入口: python3 gradio_app.py --low_vram_mode
- 踩坑:
  - Windows 官方无安装脚本，社区有 YanWenKun/Hunyuan3D-2-WinPortable（README 提及）
  - 29GB 上限对 24GB 卡不友好 → 用 --low_vram_mode 或改用 2.0 版本（2.0 总 16GB）
  - 模型权重在 Hugging Face tencent/Hunyuan3D-2.1（需接受 license）

## 二、Hunyuan3D-2.0（Tencent · 更省显存）

- VRAM（官方 README 验证）:
  - 形状：6 GB
  - 形状 + 纹理：16 GB（24GB 卡可跑）
- 模型子版本（--model_path 参数）:
  - tencent/Hunyuan3D-2mini — 小卡
  - tencent/Hunyuan3D-2mv — 中档
  - tencent/Hunyuan3D-2 — 默认
  - *-turbo 后缀 — FlashVDM 加速，推理时间减半
- 踩坑:
  - 需要 python3 setup.py install 两次（扩展编译）
  - Windows 同样依赖 WinPortable 社区工具

## 三、ACE-Step（音乐生成 · 中文歌词友好）

- VRAM（官方 README 验证）：最大 8 GB（消费级 3060/4060 可跑）
- 速度（官方 README 验证，RTF = 生成 1 分钟音乐所需时间）:
  - RTX 4090：34.48x RTF -> 1.74 秒
  - A100：27.27x RTF -> 2.20 秒
  - RTX 3090：12.76x RTF -> 4.70 秒
- CLI 入口（已验证）:
  acestep --torch_compile true --cpu_offload true --overlapped_decode true
- 中文支持：v1 内置中文歌词 LoRA + RapMachine（中文说唱 LoRA 开源）
- 踩坑:
  - 权重 ACE-Step/ACE-Step-v1-3.5B（3.5B 参数）
  - 无 Windows 原生二进制，需 WSL2 或 conda
  - ComfyUI 节点已有（ComfyUI_ACE-Step，2025.05.08 发布）-> 备选路径

## 四、TRELLIS.2（Microsoft · 图片转 3D）

- 参数规模（README 验证）：4B 参数，16x 空间下采样稀疏 voxel VAE
- VRAM（README 验证）：至少 24 GB（A100/H100 已验证）
- 分辨率：512^3 - 1536^3（高于 Hunyuan3D 的 1024^3）
- 依赖:
  - Python 3.8+，PyTorch 2.6.0 + CUDA 12.4
  - Conda 推荐（setup.sh 支持 --basic / --flash-attn）
  - flash-attn 后端，V100 等老卡需 xformers
- 踩坑:
  - 4B 参数 + flash-attn 编译是最大坑（首次约 30 分钟）
  - 输出格式：原生支持 glb / obj（与 YSM 兼容）
  - 权重 microsoft/TRELLIS.2-4B 开源下载

## 五、BSM/YSM 转换（Minecraft 模型生态 · 无人之境）

- BlockBench Dev 工具垄断：bsm-tools 仓库不存在（404 已验证）
- 现状（社区共识，无 GitHub 主仓库）:
  - BSM/YSM 格式规范无独立 open repo，依赖 BlockBench 源码（MIT）解析
  - YSM = Yasf 格式的 Model JSON + 贴图，由 BlockBench 1.x 输出
  - GLB 转 BSM 无现成工具（已验证 bsm-tools 404）
- 路线判断：glb-to-bsm 需要自研（fork MIT BlockBench 源码 + 改造，约 500-1500 LOC）

## 六、Minecraft Skin 64x64（无人之境）

- 已有开源项目：misode/mcmeta（纯数据，含 skin 字段定义）
- AI 皮肤生成：GitHub 搜索 minecraft skin AI 无人做（调研结论）
- 路线：自研 mc-skin-palette（Rust, ~300 LOC），输入角色形象 prompt + palette hex，输出 64x64 PNG（slim / classic），依赖 image crate + ComfyUI subprocess

## 七、代码生成 MCP（F9 · lilyco_agent 接入）

- 参考：modelcontextprotocol/servers（参考实现）、modelcontextprotocol/registry（注册表）
- 踩坑：MCP server 必须独立 stdio 进程（不能嵌入 lyco-cli）
- 路线：lyco-codegen-mcp（Rust, ~500 LOC）暴露给 lilyco_agent / Claude Code / Cursor

## 八、统一运行拓扑（24GB 卡）

| 模型 | 显存 | 24GB? | 优先级 |
|------|------|-------|--------|
| ACE-Step | 8 GB | 可 | 音乐主力 |
| Hunyuan3D-2.0 | 16 GB | 可 | 3D 主力 |
| TRELLIS.2 | 24 GB | 边界 | 高保真备选 |
| ComfyUI (SDXL) | 12 GB | 可 | 图像主力 |
| Hunyuan3D-2.1 | 29 GB | 不可 | 跳过 |

结论：24GB 卡下 ACE-Step + Hunyuan3D-2.0 + ComfyUI 串行 subprocess 可行；TRELLIS.2 单跑，不与 Hunyuan 同时加载。

## 九、待验证项（标记 TODO 给 DEEP-DIVE）

- [ ] Tencent 云 Hunyuan3D API 实际报价（官方文档未公开单价）
- [ ] ComfyUI + ACE-Step 节点组合是否可并行（同卡 8+12=20GB 内存压力）
- [ ] glb-to-bsm 参考 sample（找 2-3 个 BlockBench 导出的 .bsm 样例文件结构）
- [ ] ACE-Step 中文歌词 LoRA 的中文 ASR 准确率
- [ ] TRELLIS.2 的 --resolution 1536 实际生成时间（vs 512）


## 十、腾讯云 Hunyuan3D API 调研结论（TODO 已部分验证）

- **文档 URL 验证（本次执行）：**
  - 105718 URL 实际指向云解析 DNS，**之前 01-candidates.md 中该条 URL 标注有误**（标题匹配命中 105716 路径）
  - 腾讯云搜索页 (cloud.tencent.com/search?q=hunyuan3d) 返回空结果
  - 腾讯云产品 code 1729 系列页均为 SPA 渲染，**无静态价格表**
- **结论：Hunyuan3D API 报价需登录腾讯云控制台「费用中心 → 产品定价」获取单价**，本仓库 DEEP-DIVE 阶段再做
- **推荐路径**：本地 Hunyuan3D-2.0 (16GB) 作主力，API 作弹性扩展，避免依赖单一计费源

*— 02 完成 —*