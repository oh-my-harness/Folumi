# llm-tutor 开发规范

## 仓库结构

```
crates/
├── tutor-tools/   # RAG 检索、Web 搜索、代码执行工具
├── tutor-agent/   # Chat、CodeExec、Organize 等 Agent 能力与 CLI 入口
└── tutor-web/     # Web API（Axum，流式响应）
```

## tutor-agent 核心模块

| 文件 | 职责 |
|------|------|
| `capability.rs` | CapabilityRouter：Chat / CodeExec / Organize 能力路由 |
| `llm_provider.rs` | LlmConfig：多 provider 配置（Anthropic / DeepSeek / OpenAI） |
| `governance.rs` | GovernanceConfig：budget + audit + approval 组装 |
| `chat.rs` | 普通对话与工具调用 |

## 测试规范

mock 测试通过 `LlmConfig::anthropic("mock-model", "")` + `.with_client(mock_client)` 注入，所有 harness 共享同一个 `MockLlmClient` 实例（responses 按调用顺序消费）：

```rust
fn make_router(responses: Vec<MockResponse>, governance: GovernanceConfig) -> CapabilityRouter {
    let client = Arc::new(MockLlmClient::new(responses));
    let env = Arc::new(NoOpEnv) as Arc<dyn ExecutionEnv>;
    let llm = LlmConfig::anthropic("mock-model", "");
    CapabilityRouter::new(env, llm, governance).with_client(client)
}
```

独立 Deep Solve 能力已于 2026-07-16 退役。复杂问题由普通 Chat 或
Tutor 对话处理，必要时调用检索、网页和代码工具；旧会话轨迹仅保留只读兼容。

## 定位与原则

本仓库是 demo 仓库，目标是验证 `llm-harness-core` / `llm-harness-runtime` 的完善程度和可用性，验证方式是实现 deeptutor 的部分功能。

**如果开发过程中发现底层库功能残缺，不要在本仓库打补丁，应更新底层库，再回到本仓库使用修复后的版本。**

底层库路径（相对于本仓库）：

- `../llm-harness-core`
- `../llm-harness-runtime`

deeptutor 参考实现路径（绝对路径）：`/Users/hhl/Documents/projs/DeepTutor`

## 注意

`HarnessHooks::should_stop` 不要设为 budget adapter——budget adapter 在未超限时返回 `false`（继续），会导致无限循环。保持 `None`（默认：非工具响应后自动停止）。

## 收尾

每次开发完成后：提交并推送本仓库变更；如有进度变化同步更新顶层 `STATUS.md` 并推送 `oh-my-harness/oh-my-harness`。
