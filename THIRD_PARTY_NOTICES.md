# 第三方组件声明

## FFmpeg 7.1.5（`n7.1.5-1-g7d0e842004-20260630`）

Windows 安装包包含独立运行的 `ffmpeg.exe`，用于在导入 MP4 后提取第 0 秒画面，以及按需恢复视频的默认首帧封面；输出为工作区内的 PNG 预览封面。主程序通过参数数组启动该进程，不加载或链接其库。高级关键帧工具仍只运行用户显式选择的 FFmpeg。

- 构建来源：BtbN/FFmpeg-Builds 月末长期保留发布 `autobuild-2026-06-30-13-34` 中的 `ffmpeg-n7.1.5-1-g7d0e842004-win64-lgpl-7.1.zip`
- ZIP SHA-256：`ec1c6ae03fab10f316344973f83c549b4b662ec3d73f1658353ab1587f4cf727`
- `ffmpeg.exe`：108,298,752 B，SHA-256 `e46b5b83f8a5ff3790cc27ad1d8947146c0a22661f0337f701769f6d75bf998a`
- FFmpeg 源码：<https://github.com/FFmpeg/FFmpeg/tree/7d0e842004>
- 构建脚本：<https://github.com/BtbN/FFmpeg-Builds>
- 许可证：LGPL-3.0-or-later；详情见 <https://ffmpeg.org/legal.html>
- 上游保留边界：BtbN 月末构建保留两年；应在 2028-06-30 前重新锁定并验收新的月末构建，或迁移到长期受控镜像。

若将本项目的安装包分发给第三方，发布者必须按 FFmpeg 许可证要求一并提供与所携带二进制相对应的源码、构建说明和许可证文本。
