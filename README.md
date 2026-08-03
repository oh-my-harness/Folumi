# Folumi

**Folumi** 是一个基于
[`llm-harness-runtime`](https://github.com/oh-my-harness/llm-harness-runtime)
构建的本地优先个人知识助手。它围绕知识来源、Markdown 笔记和用户可控记忆，帮助用户与自己的长期资料持续对话。

当前代码由 `llm-tutor` 演进而来，产品界面和运行路径已经收缩为单一助手、知识来源、Markdown 笔记与用户可控记忆。旧 Tutor 连续性可经用户预览后迁入 Assistant Continuity，Quiz 与旧 Tutor 定义则通过一次性归档导出保留。

> 当前版本：`0.3.5`
>
> 当前阶段：Folumi 产品收缩基线已完成；后续迭代聚焦知识库管理与个人助手体验。
>
> 文档状态：已按 2026-08-03 的一级工作区调整更新。

## 文档

- [使用手册](./MANUAL.md)：首次配置和全部用户功能
- [产品需求规格](./docs/specs/2026-06-26-product-requirements-spec.md)
- [一级工作区决策](./docs/specs/2026-08-03-primary-workspaces-decision.md)：Knowledge Base、Notebook、Memory 的长期产品边界
- [Folumi 产品收缩计划](./docs/plans/2026-07-31-personal-knowledge-assistant-product-contraction-plan.md)
- [旧产品数据迁移清单](./docs/migrations/2026-08-01-legacy-product-data-inventory.md)
- [桌面发布计划](./docs/plans/2026-06-28-tauri-desktop-release-plan.md)
- [桌面 QA 清单](./docs/qa/desktop-release.md)
- [框架反馈](./docs/framework-feedback.md)
- [开发原则](./AGENTS.md)
- [更新记录](./CHANGELOG.md)

## 目标产品边界

| 领域 | Folumi 提供的能力 |
| --- | --- |
| Assistant | 基于 runtime session 的多轮对话，按需读取知识、引用来源并使用明确启用的工具。 |
| Knowledge Base | 导入和管理只读资料，支持 RAG 检索、原文读取、证据引用和来源定位。 |
| Notebook | 用户拥有的 Markdown 内容，可记录、管理、查看、链接、导入和导出；助手产出只有经用户确认才写入。 |
| Memory | 可独立查看、开关、修改和遗忘的助手记忆；业务可以选择完全不用或自行组织。 |
| Settings | 模型、embedding、搜索、数据目录、工具权限、外观和 Notebook 存储。 |

目标主导航包含 **Assistant、Knowledge Base、Notebook、Memory、Settings**。Knowledge Base 只管理用于 RAG 的只读来源；Notebook 是记录、管理和查看 Markdown 笔记的独立工作区；Memory 是独立的长期记忆控制面。三者保持清晰的数据边界。Quiz、Space、Student Profile、多 Tutor 和独立 Research 模式属于冻结范围。

## 快速开始

### 桌面安装包

发布产物由 [GitHub Actions](./.github/workflows/release-desktop.yml) 构建：

- `Folumi-v<version>-windows-x64-setup.exe`
- `Folumi-v<version>-windows-x64.msi`
- `Folumi-v<version>-macos-x64.dmg`
- `Folumi-v<version>-macos-arm64.dmg`

版本标签发布后可从项目的
[GitHub Releases](https://github.com/oh-my-harness/Folumi/releases)
获取对应产物。桌面应用会自动启动本地后端，无需另行运行服务。

### 开发环境

推荐环境：

- Rust stable，Rust 2024 edition
- Node.js 22
- Tauri CLI 2.x
- Protobuf 编译器 `protoc`
- 至少一个可用的 LLM API Key

Windows 可通过 Chocolatey 安装 Protobuf：

```powershell
choco install protoc -y
```

安装前端依赖：

```powershell
npm ci --prefix web-ui
```

### 启动桌面开发模式

```powershell
cargo tauri dev
```

该命令会构建后端、启动 Vite，并由 Tauri 拉起 `tutor-web` sidecar。

### 启动浏览器开发模式

终端一：

```powershell
cargo run -p tutor-web
```

终端二：

```powershell
npm run dev --prefix web-ui
```

访问 `http://127.0.0.1:5173`。后端默认监听 `127.0.0.1:8080`。

## 首次配置

进入应用左侧“设置”：

1. 在“LLM”中添加 OpenAI-compatible 或 Anthropic Messages 配置，并运行连接测试。
2. 如需知识库，在“嵌入模型”中配置并测试 embedding 服务。
3. 如需稳定联网研究，在“搜索”中配置搜索服务。
4. 在“笔记本”中决定使用应用本地 Notebook，还是绑定外部 Markdown Vault。
5. 在“能力”中设置会话预算和工具审批策略。
6. 在“外观”中选择界面语言和浅色/深色主题。

详细字段和操作流程见 [MANUAL.md](./MANUAL.md)。

## 架构

```text
Folumi desktop
  -> Tauri shell
      -> React / Vite UI
      -> managed tutor-web sidecar
          -> REST + WebSocket
          -> runtime sessions
          -> tutor-agent
              -> assistant chat / code execution
              -> research task / memory maintenance workflows
              -> runtime Knowledge search / read / evidence validation
              -> runtime Memory write / forget / approval
              -> llm-harness-runtime / llm-harness-agent
          -> tutor-tools
              -> web_search / web_fetch
              -> code_exec
          -> tutor-rag
              -> LanceDB + embedding retrieval
          -> local product stores
               -> Sources / Notes / User Memory / Sessions / Settings
```

### 工作区结构

```text
crates/tutor-agent   Agent 对话、提示词及 Research/Memory workflow。
crates/tutor-tools   Web 搜索、抓取和代码执行工具。
crates/tutor-rag     LanceDB 入库、runtime Knowledge source 和 embedding 集成。
crates/tutor-web     Axum API、WebSocket、session 映射和产品数据存储。
src-tauri            Tauri 桌面壳和 sidecar 生命周期管理。
web-ui               React 19、Vite 8、Tailwind CSS 前端。
docs                 产品规格、计划、QA 和框架反馈。
scripts              开发、版本、桌面构建和 QA 脚本。
```

## 数据存储

浏览器/源码开发模式当前仍使用旧数据目录名：

```text
<repo>/.llm-tutor/
```

当前版本暂不改目录名、应用标识符或持久化键，避免已有用户的数据被操作系统视为另一套应用数据。旧产品数据通过显式的一次性迁移或归档导出处理，详见[旧产品数据迁移清单](./docs/migrations/2026-08-01-legacy-product-data-inventory.md)。

可通过环境变量或后端参数覆盖：

```powershell
$env:LLM_TUTOR_HOME="D:\TutorData"
cargo run -p tutor-web -- --data-dir "D:\TutorData"
```

桌面发布版使用操作系统应用数据目录，Tauri 启动时把该路径传给 sidecar。应用内可在“设置 > 能力”查看并打开准确目录。

主要数据：

```text
settings.json
sessions/
knowledge-bases.json
quizzes.json
notebook/
memory/
tutors/
rag/
workflow-sessions/
```

当前 API Key 保存在本地 `settings.json`，尚未接入系统钥匙串。不要提交或共享 `.llm-tutor/` 和桌面应用数据目录。

## 测试

Rust workspace：

```powershell
cargo test --workspace -j 1
```

Agent mock integration：

```powershell
cargo test -p tutor-agent --test mock_integration -j 1
```

后端 API/store：

```powershell
cargo test -p tutor-web --lib -j 1
```

前端：

```powershell
npm test --prefix web-ui
npm run build --prefix web-ui
```

真实 Provider 集成测试需要 API Key，默认可能被忽略。

## 桌面构建与发布

本地 Windows 构建：

```powershell
.\scripts\build-desktop.ps1
```

只构建 release 可执行文件：

```powershell
.\scripts\build-desktop.ps1 -NoBundle
```

指定 bundle：

```powershell
.\scripts\build-desktop.ps1 -Bundles nsis
```

自动化 smoke QA：

```powershell
.\scripts\qa-desktop.ps1
```

版本同步：

```powershell
.\scripts\bump-version.ps1 0.2.2
```

GitHub 发布工作流在 `v*` 标签和手动 `workflow_dispatch` 下构建 Windows x64、macOS x64 和 macOS arm64 产物。CI 需要 `PRIVATE_DEPS_TOKEN` 读取私有 Git 依赖。

## 当前限制

- 单用户、本地优先；没有账号、权限、云同步或协作。
- 临时资料仍等待 runtime 提供一等 `ContextAttachment` 契约；当前不建设平行注入系统。
- 运行中的 workflow 在应用进程重启后尚不能保证从中断点续跑。
- API Key 暂存于本地 JSON，系统钥匙串尚未实现。
- Linux 安装包和自动更新尚未实现。
- RAG 切分、引用验证和桌面安装包 QA 仍需持续完善。
- 旧 Tutor 连续性需要用户显式选择后迁入 Assistant Continuity；Quiz 与 Tutor 定义只提供归档导出，不再参与运行。

更多用户侧说明见 [使用手册](./MANUAL.md)。

## 开发原则

项目优先使用 `llm-harness-runtime` / `llm-harness-agent` 提供的 session、上下文、工具编排、hook、trace、compaction 和 provider 行为。Folumi 聚焦产品数据与 UI，不在仓库内建立平行 runtime。

完整原则见 [AGENTS.md](./AGENTS.md)。框架缺口记录在 [docs/framework-feedback.md](./docs/framework-feedback.md)。

## License

MIT
