# AI Gallery

AI Gallery 是一个本地优先的 Windows 桌面作品库，用于整理 AI 图片、视频、提示词、生成项目和分类信息。作品与工作区数据保存在用户选择的本地目录中。

## 功能

- 导入图片和 MP4 视频，可复制到工作区或引用原文件。
- 按项目、作品、提示词、备注、分类和标签整理与检索内容。
- 为作品和项目维护中英文提示词、模型、平台、生成参数及备注。
- 使用回收站、备份和恢复管理数据；应用不会默认删除原始媒体。
- 创建分类项目或画布项目，整理输出作品与参考图片的关系。
- 选择单个或多个作品/项目导出完整内容，或将提示词单独导出为 JSON。
- 使用可配置的 AI Provider 生成分类建议；建议需要用户审核后才写入正式分类。
- 使用媒体完整性工具生成视频封面、检查重复文件并修复丢失路径。

## 源码开发版

本仓库发布源码，不包含维护者自用的安装包。运行开发版需要先安装 Node.js、Rust、Windows C++ 编译工具和 SDK；依赖包与 FFmpeg 按下面的命令获取。

### 环境要求

- Windows x64
- Node.js 22 或更高版本；项目锁定 npm 11.6.2
- Rust stable MSVC 工具链和 Cargo
- Visual Studio C++ Build Tools（含 MSVC x64/x86 工具）与 Windows SDK
- Windows 10 1803 及以上版本或 Windows 11（含 WebView2 Runtime）；较旧 Windows 需安装 Microsoft Edge WebView2 Evergreen Runtime

### 安装开发环境

在 PowerShell 中执行以下命令。安装完成后重新打开终端：

```powershell
# Node.js LTS
winget install --id OpenJS.NodeJS.LTS --exact

# Visual Studio C++ Build Tools、推荐组件和 Windows SDK
winget install --id Microsoft.VisualStudio.BuildTools --exact --override "--passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"

# Rust stable MSVC 工具链
$rustup = Join-Path $env:TEMP "rustup-init.exe"
Invoke-WebRequest -Uri "https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe" -OutFile $rustup
& $rustup -y --default-toolchain stable-x86_64-pc-windows-msvc

# 将 npm 固定到项目要求的版本
npm install --global npm@11.6.2
```

Node.js、Rust 和 Visual Studio Build Tools 的安装方式可参考 [Node.js LTS](https://github.com/microsoft/winget-pkgs/tree/master/manifests/o/OpenJS/NodeJS/LTS)、[Rust 安装说明](https://rust-lang.org/tools/install/)和 [Rust 的 Windows MSVC 前置条件](https://rust-lang.github.io/rustup/devel/installation/windows-msvc.html)。

Tauri 使用 Microsoft Edge WebView2 显示桌面界面。Windows 10 1803 及以上版本和 Windows 11 已包含 WebView2；其他系统请从 [Microsoft WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) 安装 Evergreen Runtime。更多前置条件见 [Tauri 官方 Windows 指南](https://v2.tauri.app/start/prerequisites/)。

### 启动开发版

```powershell
npm ci
npm run desktop:dev
```

`npm ci` 根据锁文件安装前端依赖；首次构建时 Cargo 会按 `Cargo.lock` 获取 Rust 依赖。Tauri 的开发和构建命令会自动运行 `npm run media:prepare`，下载项目所需的固定版本 FFmpeg，并校验压缩包和可执行文件的 SHA-256。需要单独准备或重新校验 FFmpeg 时可手动执行 `npm run media:prepare`。依赖目录和 FFmpeg 可执行文件都不会提交到仓库。

根目录的 `启动作品库.cmd` 是开发启动脚本，会运行 `npm run desktop:dev`。若本机已有通过校验的 FFmpeg，Tauri 前置脚本会直接复用，无需手动重复准备。

### 构建与检查

```powershell
npm run typecheck
npm run check
npm run desktop:build:check
```

`npm run desktop:build:check` 会编译桌面应用但不生成安装包。`npm run desktop:build` 会在本机生成 Windows NSIS 安装包；该产物用于本地构建验证，不纳入源码仓库。

GitHub Actions 会在 Windows x64 云端环境执行锁文件安装、完整代码检查和桌面编译；CI 不上传安装包。

`npm run check` 包含格式检查、TypeScript 类型检查、前端测试与构建、Rust 格式检查、测试和 Clippy。

## 项目结构

```text
src/                 React 页面、组件、前端服务与样式
src-tauri/src/       Rust 命令、领域模型、服务、Repository、Adapter 与数据库迁移
src-tauri/capabilities/ Tauri 权限配置
docs/                用户指南、产品状态、进度和架构决策
tests/               前端与服务测试
scripts/             本地构建辅助脚本
```

详细使用步骤见[用户指南](docs/USER_GUIDE.md)，产品范围见 [PRD](PRD.md)。

## 数据与隐私

- 工作区由用户创建或选择，包含数据库、媒体、缩略图缓存和备份。
- 引用导入会保留原文件路径；复制导入会把媒体副本保存到工作区。
- AI 服务凭据保存在 Windows 系统凭据存储中，不写入普通工作区备份。
- 不要提交真实工作区数据库、用户媒体、工作区备份、API 密钥或本机专用配置。
- `.gitignore` 已排除构建产物、临时目录、数据库、`.env` 文件、FFmpeg 可执行文件和本地发布目录。

## 许可证

当前仓库没有 `LICENSE` 文件，尚未指定开源许可证。添加许可证前，请勿将本项目视为已授予开源使用权。
