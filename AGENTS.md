# 造隅 HarmonyOS 工程约定

- 上游 Rust 库只允许通过根 `Cargo.toml` 的 `workspace.dependencies` 使用远端
  `git` 依赖，并固定完整 `rev`；`crates/*` 通过 `workspace = true` 继承。
- 不使用 Git submodule，不把上游源码复制进工程，不用本地 `path` 或本地
  `[patch]` 覆盖上游库。本工程自有 crate 之间可以使用 workspace 内的路径依赖。
- 上游所需修复或扩展直接提交到对应远端分支，再更新依赖 revision 和锁文件。
  不维护本地补丁文件，不修改 Cargo 缓存，不在 setup/build 中应用补丁。
- 品牌名称、图标及宿主服务由本工程传给上游公开接口；保留原作者与第三方许可。
- ArkTS 上游插件修复提交到对应仓库 PR，通过 ohpm-rs 的远端 Git 依赖固定提交；
  不保留本地插件副本。
- 鸿蒙依赖使用 ohpm-rs 管理，应用通过 `tools/project.py` 调用 arkdown 打包。
- 应用设备范围为 `tablet`、`2in1`，不支持 `phone`。

- 每款编辑器只保留一个 `<product>-ability` Rust crate，内部整合 host 模块和
  Ability 注册；不得拆成独立 host / ability 两个 crate。
