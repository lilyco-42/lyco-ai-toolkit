# Lain42 Asset Studio 集成产品分发

2026-10-11：产品所有者已明确确认持有 Spine 授权，并要求继续核实打包条件。
本构建以有效且适用于商业集成的 Editor 授权为前提，不保存或分发 Editor 激活码。

核对官方当前 Editor Agreement §2.1–2.4 与 Runtime Agreement：
https://esotericsoftware.com/spine-editor-license
https://esotericsoftware.com/spine-runtimes-license

交付为完整的本地素材工作流：文件选择、AIC 解析/部件导出、动画选择、Spine 3.8 GIF
播放与导出、结果下载、取消与工作区清理。它不是单独转售 Runtime 的 SDK 或组件包。
分发包附当前 Runtime 许可、原依赖版权文本、两个源码版本和逐文件校验清单。
Runtime 不标为 MIT；本库自有 MIT 代码和 Runtime 的许可分别保留。
修改/集成包含 Runtime 的衍生产品仍须满足自己的 Editor 授权要求。

独立 Actions 工作流 `asset-studio-licensed.yml` 用准确的公共应用 SHA 和本库 SHA 构建，
不公开本库的研究文件或其他私有源码。没有输出单独 Runtime 的公共 Release。
Windows / Linux 均使用实际 CLI 读取自制色块样例，检查不同动画帧、透明像素和 GIF 导出；
Linux 另有桌面/手机尺寸浏览器验收；打包后再次调用两个真正的内置引擎。
仅在这些门及相应代码 CI 通过后，发布完整产品包；PR 构建不冒充正式版本。

当前普通 CI 发现既有 `spine normalize --pma` 无条件删除 pma，虽然调用者并未选
`--strip-3-8`。已修复为只在显式 3.8 转换时删除，并增加命令级回归检查；未知 PNG
尺寸提示的旧文本断言同步到实际说明。没有改动解析器或渲染逻辑。

当前默认授权整合构建固定公共 main `a308ad03671a2b8d415b85ff2d7e2e7bfc42796b`，
禁止用 PR 的 merge sample 冒充正式交付；更新包尚需当前正式门全部通过。
CI 先完整保存 Spine 信息输出再检查内容，避免 `grep -q` 提前关闭管道导致 SIGPIPE 误报。

## 原生界面与安装器门禁

0.1.1 组合包固定公共正式 main `a308ad03671a2b8d415b85ff2d7e2e7bfc42796b`，补齐
Microsoft.Web.WebView2 SDK 1.0.3856.49 独立 BSD-3-Clause 许可、官方来源与五个 DLL 摘要。
公共应用的 notice collector 对照官方 NuGet 包核验安装依赖与最终冻结 DLL；授权组合工作流
再把原始 SDK LICENSE.txt 和来源 pin 放进 Windows ZIP／安装 EXE，完整 manifest 覆盖它们。
SDK 与运行时为不同组件，SDK 署名不能代替 Spine Runtime 的独立许可，两者同时保留。
正式组合包需公共 main 及本库当前源码的全部门通过；候选 PR 包不作为正式交付。

界面使用同一个 React/shadcn 静态产物，Windows WebView2 不暴露 Python API。安装器为当前用户安装，卸载仅删除包内明确列出的文件。独立产品 CI 也必须通过真实 WebView2 的 AIC 预览、导出与正常关闭清理，安装后的完整应用再次验证 AIC + Spine，然后卸载并验证用户自存文件保留。构建、前端检查与全部测试只在 GitHub Actions，未通过不发布。
