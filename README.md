# 造隅 HarmonyOS 总工程

一个 HarmonyOS 工程、多个 `entry`、一套 Rust 工作区。通过 product 选择应用，
每个应用独立生成 HAP、使用独立包名和沙箱。

| Product / entry | 应用 | 包名 | 当前内容 |
| --- | --- | --- | --- |
| `pixrune` | Pixrune · 造隅修图 | `com.richerfu.pixrune` | PhotoCraft 鸿蒙图像编辑器 |
| `vecrune` | Vecrune · 造隅矢绘 | `com.richerfu.vecrune` | VectorCraft 鸿蒙编辑器及 Rust 接入 |
| `cliprune` | Cliprune · 造隅剪辑 | `com.richerfu.cliprune` | FilmCraft 鸿蒙视频编辑器 |
| `pagrune` | Pagrune · 造隅阅编 | `com.richerfu.pagrune` | PdfCraft 鸿蒙 PDF 编辑器 |
| `motrune` | Motrune · 造隅动效 | `com.richerfu.motrune` | EffectCraft 鸿蒙合成编辑器 |
| `layrune` | Layrune · 造隅版式 | `com.richerfu.layrune` | DesignCraft 鸿蒙排版编辑器 |
| `gridrune` | Gridrune · 造隅表格 | `com.richerfu.gridrune` | GridCraft 鸿蒙电子表格 |
| `nodune` | Nodune · 造隅工坊 | `com.richerfu.nodune` | 套件目录，可打开以上已安装应用 |

DevEco 要求存在 `default` product；它是 `nodune` 的 IDE 默认入口，使用同一个
entry 和包名，不新增应用。

本次覆盖 `harmony-contrib` 下具有 `feat-openharmony` 分支的全部 7 个 Craft 仓库。
其余 10 款应用的名称、包名和待迁移状态保留在 `config/apps.json`。
同一 product 仅选择自己的 entry，各应用具有独立 bundle。

## 目录

```text
AppScope/                   共享应用资源与默认应用配置
build-profile.json5         products、entry 与产品的对应关系
hvigorfile.ts / hvigor/     DevEco 工程识别和同步配置
entries/<product>/          各编辑器 ArkTS Ability、应用资源和权限
entries/nodune/              工坊 ArkTS 页面与应用目录
packages/ability-host/      共享编辑器 Ability 与插件组合
crates/<product>-ability/  每应用一个 crate，含 Ability 注册、生命周期和 OS 服务
licenses/<upstream>/       各上游及素材许可文本，不包含依赖源码
config/apps.json           全系列应用名称与 com.richerfu.* 包名
tools/project.py           环境检查、依赖安装、Rust 编译与打包入口
dist/{debug,release}/      校验后的 unsigned HAP 和摘要，生成目录
```

ArkTS 包由 **ohpm-rs** 管理，HAP 由 **arkdown** 打包。Rust 依赖通过 Cargo 维护；
每个编辑器只有一个自有 Rust crate；依赖集中声明在根 `Cargo.toml` 的 `workspace.dependencies`。
上游引擎通过固定提交的 Cargo Git 依赖接入，源码由 Cargo 缓存管理。
本工程不包含 submodule、上游引擎/UI 库的源码副本或本地补丁应用流程。
鸿蒙 host 与 Ability 注册合并在各应用的同一个 crate 内，迁移来源记录在 `NOTICE`。

## 环境与首次准备

已验证的工具版本：Node 22.23.2、ohpm-rs 0.5.0（下方固定 Git 提交）、arkdown 0.1.6、Rust 1.99.1。
Python 需 3.11 或更新版本，Rust 最低版本 1.95。安装 `aarch64-unknown-linux-ohos` target；SDK 需包含
OpenHarmony native 工具链和目标 API 26。当前应用兼容版本沿用上游 API 22，
声明设备为平板和 2in1（`tablet`、`2in1`），不支持手机；这不代表已覆盖所有对应设备的运行测试。

```sh
git clone git@github.com:harmony-contrib/zaoyu.git zaoyu-ohos
cd zaoyu-ohos
cargo install --git https://github.com/harmony-contrib/ohpm-rs.git \
  --rev 13425765b1c0ef0e022c5df2e214603048cb1192 \
  --locked --debug --force ohpm-cli
npm ci
python3 tools/project.py setup
python3 tools/project.py doctor
python3 tools/project.py deps
```

`npm ci` 只安装固定版本的 arkdown 及其宿主二进制；鸿蒙应用依赖安装实际执行
`ohpm-rs install --workspace --all`。ohpm-rs 需包含上游
[PR #3](https://github.com/harmony-contrib/ohpm-rs/pull/3) 的修复：旧版本在已有锁文件、
但没有本地缓存时，无法恢复 Git 子目录包。上方命令固定该修复提交，`--debug` 用于
缩短工具自身的首次安装时间，不影响应用的 Debug/Release 打包模式。
若找不到 SDK，可设置：

```sh
export OHOS_NDK_HOME="/path/to/DevEco-Studio.app/Contents/sdk/default/openharmony"
```

脚本不记录这台电脑的绝对路径。优先使用项目内 arkdown，未安装时可使用 PATH
上的 arkdown。根目录、各 workspace 包的依赖锁文件均应随源码维护。

## 在 DevEco Studio 中打开

使用支持 API 26 的 DevEco Studio，选择 **Open** 并打开 `zaoyu-ohos` 根目录。
该目录包含 `AppScope`、`build-profile.json5`、`oh-package.json5`、`hvigorfile.ts`
和 `hvigor/hvigor-config.json5`；各 entry/HAR 也提供对应的 Hvigor 入口。
若此前已作为普通目录打开，关闭该项目后重新打开根目录，让 IDE 重新识别工程。

完成首次准备后，在工具栏 **Select Product** 中选择表格中的任一 product；
首次打开时的 `default` 对应 Nodune。各应用分别对应独立 entry 和包名。
Hvigor 配置用于 IDE 识别、同步和代码分析；依赖维护仍使用 ohpm-rs，完整构建仍
使用下方的 arkdown 总入口。
DevEco 自带的 Build/Run 不会自动构建 Rust；各编辑器的原生库应先通过总入口生成。

## 编译与验证

```sh
# 单个应用（自动先构建并拷贝所需的 Rust 原生库）
python3 tools/project.py build vecrune
python3 tools/project.py build gridrune
python3 tools/project.py build nodune

# 所有已接入应用
python3 tools/project.py build all

# Release；不默认启动调试控制服务
python3 tools/project.py build all --mode release

# 打包隔离测试、Rust 格式与测试、OHOS clippy、全部应用构建与包内容检查
python3 tools/project.py check
```

也可使用 `npm run build:<product>` 或 `npm run build:all`。
默认最多两个 Rust 编译任务，可通过 `--jobs N` 调整。首次编译会下载并构建编辑器
引擎、图形库和格式库，耗时明显长于后续增量构建。
构建 `all` 时使用一次 Cargo 编译共享依赖，再分别打包各应用的独立动态库。

输出为 `dist/debug/<product>-unsigned.hap`，
release 模式输出到 `dist/release/`。旁边的 JSON 保存包名、module、源码提交、
SHA-256、设备范围和大小。打包脚本验证实际 HAP manifest 的包名、entry 和
`tablet` / `2in1` 设备范围，并检查
各编辑器包内仅包含自己的原生编辑器库，且为本次构建、剥离符号后的 ARM64 ELF；
缺少原生库、混入其他编辑器库或包含手机设备类型时构建失败。
带符号的原始库仍保留在 `target/aarch64-unknown-linux-ohos/{debug,release}/`。
每次原生构建还记录源码、锁文件、图标、feature、Cargo 配置、编译参数和可选外部
字体内容的摘要；单独执行 `verify` 时，
若声明的依赖或本地 Rust 源码已改变，会要求重新构建，避免把旧包标记成新提交。
直接调用 arkdown 不会自动编译 Rust，请使用总入口脚本。

当前交付不包含个人/企业证书。生成的 HAP 未签名；安装和上架前需由发行方配置
对应包名的签名与 provisioning profile。本工程不会自动安装或启动设备上的应用。

## 上游与原生接入

源仓库为 `https://github.com/harmony-contrib/<upstream>`，各自使用
`feat-openharmony` 的适配代码。应用到源码的映射见 `config/apps.json` 的
`sourceRepository` / `upstreamCrates` / `nativeCrate` 字段。
实际固定提交见根 `Cargo.toml` 的各组 Git 依赖及 `Cargo.lock`。
各自有 crate 通过 `workspace = true` 继承，源码无需额外克隆或初始化。
统一使用带 OHOS 适配的 egui、winit 和 craft-libs 固定 revision 与 crates.io patches，
不能直接换成未带 OHOS 适配的同名官方版本。

PdfCraft 的 9 个带上游修复的 PDF 后端包也通过其仓库的固定 Git 提交获取；
后端基础提交与编辑器/UI 提交分别锁定，具体来源由 `Cargo.lock` 记录。
它们与其他编辑器使用的 craft-libs 同名后端各自保留所属分支的实现。

已迁移的行为包括：Ability 生命周期、XComponent、键盘 RESIZE 模式、持久化文件
授权、保存目录授权与原子写入、后台打开/置入、原生剪贴板及按需权限、Want 打开
文件、URL 打开、截图取色、系统中文字体、界面/偏好保存、恢复文件和 HiLog/轮转日志。
`ohos-control` 为显式的 Cargo feature，默认关闭，仅用于开发验证。可通过
`python3 tools/project.py build vecrune --features ohos-control` 开启原分支的
本机 `127.0.0.1:7878` 控制通道；普通构建不会启动该服务。

ArkTS 和 Rust 两侧桥接协议均固定为 `1.0.0-beta.5`。保存插件由 ohpm-rs 从
`harmony-contrib/openharmony-ability` 的 `plugins/files` 子目录按完整 Git 提交获取；
修复见上游 [PR #123](https://github.com/harmony-contrib/openharmony-ability/pull/123)，
本工程不保存插件源码副本。`FilesPlugin(false)` 在 API 23+
且能力支持时禁止预创建空文件，保留原子保存方式。根 `oh-package.json5` 的 override
将上游 workspace 依赖映射到已发布的 `@ohos-rs/ability@1.0.0-beta.5`，确保桥接版本一致。
不要在 `oh_modules` 中手动修改依赖。

`setup` 会验证每款应用的上游依赖来自同一完整 Git revision，并更新工坊目录。
各编辑器通过上游公开的品牌接口传入名称、图标和版权说明。
上游修复或扩展应直接提交到远端对应分支，再更新 Cargo revision、锁文件及构建摘要；
不要维护本地 patch、路径覆盖或修改 Cargo 缓存。约束记录在根 `AGENTS.md`。

## 后续增加应用

1. 在 `entries/<英文名小写>/` 添加真实 entry 和使用 `hapTasks` 的 `hvigorfile.ts`，
   并声明最小需要的权限。新增共享 HAR 使用 `harTasks`。
2. 在根 `build-profile.json5` 添加同名 product，设置
   `bundleName: com.richerfu.<英文名小写>`，用 `applyToProducts` 选中对应 entry。
3. 每款 Rust 原生应用只增加一个 `<product>-ability` crate，内部包含 host 模块和
   Ability 注册；共用 ArkTS 桥接沿用 `packages/ability-host`。新依赖集中在根 Cargo 工作区。
4. 在 `config/apps.json` 登记 `nativeCrate`、`sourceRepository`、`upstreamCrates`，
   将状态改为 `integrated`；总入口按这些字段选择构建与验证，不再硬编码应用列表。
   运行 setup 更新工坊目录，重新安装依赖并构建验证。

独立版本可通过各 product 的 `versionCode` / `versionName` 覆盖；当前产品
从 AppScope 继承 `1.0.0 / 10000`。Rust 上游版本由各自固定提交决定。

## 许可与待验证项

原始软件及素材许可文本保留在 `licenses/<upstream>/`；完整素材清单、贡献者与
源码通过 Cargo 固定的 Git revision 获取。根 `NOTICE` 记录迁移来源，`ASSETS.md`
记录替换图标来源。
`setup` 将这些许可复制到生成的 `resources/rawfile/zaoyu-licenses/`，随每个 HAP
分发；打包时检查许可存在。生成目录无需提交。
鸿蒙启动图标、窗口标题、应用内标志和 About 使用各应用独立身份；部分上游
帮助菜单/说明仍含来源项目文字，商店发布前还需做完整产品文案和素材审核。

Cliprune / Motrune 沿用上游当前鸿蒙音频限制：尚无原生音频播放后端，
Cliprune 也没有麦克风采集后端。音频预览和采集不能作为本次接入已支持的功能。
Gridrune 尚未接入鸿蒙原生打印服务。

桥接 beta.5 的 SDK 检查会报告 ESObject、API 版本、设备能力及可抛出异常等警告。
检查没有通过 `--skip-checker` 绕过。依赖 HAR 的 `module.json5` 也触发 arkdown
0.1.6 的预编译 library profile 提示；应用需要的权限由 entry 明确声明。
本次全系列集成及单 crate 合并尚未完成全套构建和新增编辑器的设备验收。
平板/2in1 上的文件授权、粘贴、输入法、前后台及复杂文档渲染仍需逐项实测。
编译及包结构通过不等于商店审核、签名或真机验收通过。
