# 移除 Research 模式的产品决策

> 状态：已接受  
> 日期：2026-08-05

## 背景

现有普通 Chat 已具备网页搜索、网页读取、知识库检索、Notebook 读取、代码执行和引用展示能力。独立 Research 入口又引入了计划确认、专用报告工具、工作流恢复和报告卡片，增加了用户选择成本与实现复杂度，但没有形成必须独立存在的产品价值。

## 决策

Folumi 不再提供独立的 Research 模式或 Research 任务动作。研究型请求直接在普通 Chat 中完成，不需要用户预先切换模式。

以下能力必须保留：

- Chat 可按任务需要使用 `web_search` 与 `web_fetch` 查证外部信息；
- Chat 可使用已授权的知识库、Notebook、Saved Memory 与 History Recall；
- 回答继续展示可导航的来源和引用；
- 普通回答可保存到 Notebook；
- 历史 `research_report` Notebook 条目仍可读取，不删除用户数据；
- 旧会话元数据中的 `research` capability 在打开时归一化为 `chat`。

以下链路退出活动产品边界：

- Research composer 入口与 Research capability；
- `propose_research_plan` 和 `create_research_report` 产品工具；
- Research 专用提示词、阶段事件、报告 artifact 与恢复逻辑；
- Research 报告专用卡片、重新生成和来源入库操作；
- Research 专用 runtime workflow 与测试。

## 防偏移规则

后续不得仅通过新增按钮或前端开关恢复 Research。若普通 Chat 无法满足某类调研任务，应优先改进 Chat 的工具选择、检索质量、引用质量和长任务体验。重新引入独立 Research 产品层级必须先形成新的产品决策，并同步更新 PRD、使用手册、QA 清单和回归测试。

## 验收标准

1. Assistant 输入框和帮助文档中不存在 Research 模式入口；
2. 新建或更新会话不能选择 `research` capability；
3. 普通 Chat 仍可调用网页、知识库和 Notebook 工具并生成引用；
4. 普通 Chat 回答仍可保存到 Notebook；
5. 旧 Research 会话可作为 Chat 打开，历史消息与旧 Notebook 条目保持可读；
6. 活动代码中不存在 Research 专用工具或 workflow 模块。
