# 项目任务进度

## In Progress

- P1 真人试用第四轮与首次源码发布：作品/项目本地导出、作品搜索字段选择、缩略图缓存失效修复和 Windows 开发说明已整理；补齐完整导出的自定义字段及作品导出的画布成员边界。源码首次发布到 GitHub 时不推送标签或安装包，`.codex/` 与本地规划文件排除；本地已有 `v0.9` 回滚标记保留但不发布。最终 Windows 云端检查结果待填。

## Todo

- [ ] P2 高级扩展：按用户要求暂不处理，不实现也不生成交接。

## Done

- [x] 发布清理与 Windows 一键安装包重封装 — 按明确路径清除约 6.51 GB 可再生构建产物、缓存、测试临时目录与过时安装包，保留锁定依赖和已校验 FFmpeg 打包输入，未触碰用户作品、数据库、媒体或密钥。`npm run check`（前端 168/168、Rust 188/188 + 5 ignored、严格 Clippy）通过；100/1,000/10,000 条发布基准与同环境 HEAD 比较无超过 20% 回退，Private Bytes 203.09 MiB。新 NSIS 安装包 38,965,317 B，SHA-256 `03B0BDA28C6634E75A689D50B5269C4FEA866F0272E1231B0DD5ADBDED464725`；交互安装器、全新静默安装、应用启动 5 秒、内置 FFmpeg 哈希和静默卸载无残留均通过；独立复审在本地自用边界下无 P0/P1，两项收口 P2 已关闭。
- [x] P1 真人试用第三轮优化 — 创作效率批量编辑支持从预设选择或输入新模型/平台，留空保持原值；项目支持分类/画布双形态，画布以独立分页维护参考图片、最终输出和 `@引用名` 提示词关系，移除卡片与详情的 `PROJECT` 英文小标题。v10 增量迁移、备份、回滚和回收站快照均保护旧项目及作品信息；`npm run check`（前端 168/168、Rust 188/188 + 5 ignored、严格 Clippy）与 locked 桌面发布构建通过；10,000 条同环境数字分页较 HEAD +0.77%，入口 JS gzip 86,446 B、画布懒 chunk gzip 2,837 B、exe 15,994,368 B，阶段性能和体积回退均低于 20%；独立复审无 P0/P1/P2。
- [x] 隐藏 NSFW 作品库 — 标题圆角框每次间隔不超过 1 秒的连续五击懒激活；独立 root/service/数据库/媒体/分类与设置，后端拒绝工作区目录重叠；退出时取消未结束导入、释放 Blob URL 和恢复主题。相关前端 83/83、TypeScript、生产构建、Rust 定点测试、格式和严格 Clippy 通过；NSFW 懒 chunk gzip 1.58 kB，独立复核无 P0/P1/P2。
- [x] P1 首轮试用反馈修复 — 图片按需缩略图与详情预览契约、视频封面复用和直达设置入口、逻辑分组的响应式筛选、10/25/50 数字分页、项目用途说明、设置路径选择/恢复新目录/AI 表单反馈及整体对齐；`npm run check`（前端 145/145、Rust 173/173 + 8 ignored、严格 Clippy）与 locked 桌面发布构建通过，100/1,000/10,000 条数字分页 0.1422/0.2389/1.0416 ms，入口 JS gzip 80.87 kB、exe 15,811,072 B；独立复审无 P0/P1/P2，未修改真实作品目录、数据库或媒体。
- [x] M8 P1 静态公开作品网站导出与 P1 收尾 — 项目/作品独立公开范围、默认关闭的字段选项、不可逆范围指纹与双重复算、原子 staging、目录身份防替换、图片去元数据重编码、视频点击加载、100 条静态分页和 `/export` 懒路由；`npm run check`（前端 129/129、Rust 170/170 + 8 ignored、严格 Clippy）通过，100/1,000/10,000 条默认完整导出 249.528/2,437.733/68,187.438 ms，入口 JS gzip 79,542 B、导出懒 chunk 2,558 B、exe 15,757,312 B，隐藏启动 5 秒和三轮独立复审修复后无 P0/P1。
- [x] M7 P1 媒体与完整性 — v5 原子迁移与累计 schema 校验、视频关键帧/自定义封面、完全重复检测补强、单文件丢失路径修复、只读/确认/脱敏边界；`npm run check`（前端 119/119、Rust 153/153 + 5 ignored、严格 Clippy）通过，100/1,000/10,000 条重复检测 0.195/0.425/2.955 ms，入口 JS gzip 78,455 B、P1 懒 chunk 9,292 B、媒体工具懒 chunk 2,942 B、exe 15,587,328 B，隐藏启动 5 秒和独立复审修复后无 P0/P1。
- [x] M6 P1 效率工作流 — v4 原子迁移与旧备份 staging 升级、批量编辑/分类/AI、模型对比、版本化智能集合、真实游标分页、自定义字段、提示词版本及真实修改历史；`npm run check`（前端 112/112、Rust 134/134 + 4 ignored、严格 Clippy）通过，100/1,000/10,000 条分页 0.0900/0.0895/0.0961 ms，干净入口 JS gzip 78,450 B、P1 懒 chunk 7,940 B、exe 15,238,656 B，隐藏启动 5 秒和独立复审无 P0/P1。
- [x] M5 迁移与发布（M5-001~M5-003）— 完整/轻量备份与新目标恢复、便携工作区验证、前后端只读展示模式、NSIS 安装包及 P0 总验收；`npm run check`（前端 99/99、Rust 100/100，4 项发布基准按需执行）通过，100/1,000/10,000 条搜索 0.3300/1.2367/6.0162 ms，独立复审无 P0/P1。
- [x] BOOT-001 核对 PRD、环境、Git 与 Agent 现状 — 文档检查及运行时命令
- [x] BOOT-002 建立治理文档、目录和 Agent 配置 — 文件结构检查待完成
- [x] PLAN-001 制定依赖有序的 P0 实施顺序与 M1-001 验收 — planner 只读复核
- [x] M1-001 React/Vite 严格模式应用壳与构建基线 — `npm ci`; format/typecheck; 7/7 tests; build; performance_agent 通过; review_agent 无高优先级问题
- [x] M1-002 Tauri 2 最小桌面壳、只读 typed command 与安全错误边界 — `npm run check`（前端 14/14、Rust 4/4、Clippy）；`npm run desktop:build:check`；隐藏窗口 5 秒存活；performance_agent 无 >20% 回退；review_agent 无未处理阻断
- [x] M1-003 工作区创建/打开、可迁移路径与轻量检查 — `npm run check`（前端 33/33、Rust 22/22 + 1 ignored benchmark、Clippy）；`npm run desktop:build:check`；100/1,000/10,000 文件打开约 0.19–0.20 ms；隐藏启动 5 秒；performance_agent/review_agent 无阻断
- [x] M1-004 SQLite v1 migration、一致备份与事务回滚 — `npm run check`（前端 46/46、Rust 32/32 + 2 ignored benchmarks、Clippy）；locked no-bundle release；100/1,000/10,000 条轻量打开均约 1.1 ms；test/performance/review Agent 独立验收，无剩余阻断
- [x] GOV-001 将后续流程调整为阶段内异步并行、阶段末统一测试/性能/审查 — `AGENTS.md` 与 ADR-LITE-010
- [x] M2 本地作品库（M2-001~M2-006）— `npm run check`（前端 78/78、Rust 57/57 + 3 ignored、Clippy）；locked release 与隐藏启动冒烟；100/1,000/10,000 条分页均约 0.1 ms；performance/review Agent 独立验收通过
- [x] M3 检索与效率（M3-001~M3-002）— v1→v2 FTS migration/备份回滚、组合筛选与重复组、按需缩略图 LRU、原生虚拟网格；`npm run check`（前端 85/85、Rust 67/67 + 4 ignored、Clippy）；locked no-bundle release；10,000 条 FTS 基准 4.9544 ms；独立审查复验无 P0/P1
- [x] M4 AI 分类（M4-001~M4-002）— v2→v3 migration/备份回滚、可插拔 OpenAI 兼容/Gemini/Ollama adapter、系统安全存储、字段白名单和发送预览、pending 审核/接受/拒绝/修改状态机、分类去重/合并/事务写入、设置与审核懒路由；`npm run check`（前端 90/90、Rust 83/83 + 4 ignored、Clippy）；locked release；100/1,000/10,000 条数据库打开 1.660/1.697/1.862 ms，搜索 0.1772/0.5886/4.7820 ms；独立复审无 P0/P1
