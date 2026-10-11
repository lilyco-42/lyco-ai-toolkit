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
