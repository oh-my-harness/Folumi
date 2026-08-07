# Folumi 个性化系统（内部 Memory）重设计

> 状态：已接受（Accepted）——Phase 1 Saved Memory 与 Phase 2 History Recall 已实现并通过离线验收，受控增强项进入 Phase 3
>
> 决策日期：2026-08-03
>
> 最近修订：2026-08-07——用户界面改用“个性化 / 关于我 / 个人信息 / 参考过往对话 / 助手设定”，内部 Saved Memory、History Recall 与 Assistant Profile 契约保持不变
>
> 替代范围：早期设计与实施计划中描述的 L1/L2/L3 记忆模型及记忆整理工作流

## 1. 背景

Folumi 已将“个性化”作为独立、由用户控制的一级工作区。该名称表达“助手如何认识用户、延续对话并表现自己”，避免与用户主动撰写的笔记混淆。内部仍使用 Memory 领域契约。旧后端采用隐藏活动日志（L1）、生成摘要（L2）、用户档案文件（L3），再通过维护任务在不同层级间整理内容。这套模型会记录用户并未要求记住的活动，把文件位置变成业务模型的一部分，也让界面无法如实说明系统究竟保存了什么。

旧分层链路已经退役：

- Assistant、Notebook 和知识库操作不再记录 L1 活动事件；
- 不再读取或写入 L2 摘要与 L3 档案文件；
- 不再保留整理预览、运行、应用、撤销和文件编辑 API；
- 旧分层文件不迁移、不导入，也不会被应用自动发现、修改或删除。

清理旧链路之后，Folumi 需要的不是另一套隐藏层级，而是几种权威来源清楚、用途不同、均可由用户控制的连续性能力。

## 2. 设计结论

新系统采用“保存的记忆 + 历史检索”双通道，并继续把助手配置作为独立数据域：

1. **当前会话上下文**：由 `llm-harness-runtime` Session 负责，是本次对话的直接上下文。
2. **保存的记忆（Saved Memory）**：用户明确新增，或助手从用户直接陈述中识别并按授权规则写入的长期条目，可跨会话召回。
3. **历史检索（History Recall）**：在用户选择开启后，仅当用户明确询问或引用旧对话时，Agent 才通过可见工具从既有 runtime Sessions 中按需查找原始对话；首版不在 run 前隐式自动召回，检索结果也不会自动晋升为保存的记忆。
4. **助手配置（Assistant Profile）**：助手的名称和行为说明，由产品设置保存；它描述助手，不描述用户，因此不属于保存的记忆。

这些是不同的**数据权威与使用通道**，不是 L1/L2/L3 那样的物理存储层级。界面和 API 不暴露文件路径、整理任务或隐藏的层间同步。

```text
当前输入
  ├─ 当前 runtime Session ───────────────> 本轮直接上下文
  ├─ 保存的记忆（显式、可编辑）──────────> 稳定偏好与连续性
  ├─ 历史检索（可选、按需工具读取）──────> 相关旧对话片段
  └─ Assistant Profile ─────────────────> 助手身份与行为约束
```

## 3. 设计依据

本方案吸收成熟产品和 Agent 框架中已经形成共识的部分，但不照搬其云端数据模型：

- ChatGPT 将保存的记忆与聊天历史引用视为不同能力，并提供关闭、删除来源聊天和临时对话控制；
- Claude 提供跨聊天检索，并按 Project 隔离记忆空间，允许暂停、重置和查看来源；
- Gemini 允许基于历史聊天个性化，并让用户通过关闭功能、删除来源聊天或在对话中纠正来控制结果；
- GitHub Copilot 区分用户偏好与仓库范围事实，并对仓库事实进行来源引用、重新验证和过期清理；
- LangGraph 将线程内短期状态与跨线程长期存储分开，并区分语义、情节和程序性记忆；
- Letta 的可见 Memory Blocks 说明结构化、可查看的核心记忆适合进入上下文，但不意味着所有历史都应复制为记忆条目。

参考资料：

- [OpenAI Memory FAQ](https://help.openai.com/en/articles/8590148-memory-faq)
- [Claude chat search and memory](https://support.claude.com/en/articles/11817273-use-claude-s-chat-search-and-memory-to-build-on-previous-context)
- [Gemini personalization from past chats](https://support.google.com/gemini/answer/16598469?hl=en)
- [GitHub Copilot Memory](https://docs.github.com/en/enterprise-cloud@latest/copilot/concepts/agents/copilot-memory)
- [LangGraph Memory](https://docs.langchain.com/oss/python/concepts/memory)
- [Letta Memory Blocks](https://docs.letta.com/v1-sdk/memory/memory-blocks)
- [Mem0: Building Production-Ready AI Agents with Scalable Long-Term Memory](https://arxiv.org/abs/2504.19413)

## 4. 产品边界

| 数据域 | 权威来源 | 主要用途 | 是否可被 Agent 修改 |
| --- | --- | --- | --- |
| 当前会话 | runtime Session | 当前对话连续性 | 由正常会话生命周期追加 |
| 保存的记忆 | Folumi Memory store | 跨会话的稳定事实、偏好、目标和连续性 | 用户明确操作，或助手按用户授予的写入权限操作 |
| 历史检索 | runtime Sessions | 找回相关旧对话 | 只读；不能改写历史 |
| 助手配置 | Product settings | 助手名称与行为说明 | 用户在“助手配置”中修改 |
| Sources | Knowledge Base 原始资料 | RAG 事实依据 | 对 Agent 只读 |
| Notes | 用户拥有的 Markdown | 用户知识成果 | 明确授权后修改 |

以下边界必须保持：

- 打开、阅读或编辑 Notebook、知识库资料不会自动创建保存的记忆；
- 历史检索不会复制 Session 正文到 Memory store，也不会自动生成长期条目；
- 保存的记忆只用于个性化与任务连续性，不能替代知识库引用或外部事实证据；
- Assistant Profile 不得混入用户事实；可复用的助手策略和行为规则属于 Assistant Profile，而不是用户记忆；
- 产品不重建 Session、上下文构建、compaction、工具编排或持久化协议，继续优先使用 runtime 能力。

### 4.1 助手身份与提示词所有权

Folumi 的最终 system prompt 由三个职责不同的层次组成：

| 层次 | 所有者 | 可以包含 | 不得包含 |
| --- | --- | --- | --- |
| Core Policy | 产品代码与 runtime 能力边界 | 安全、权限、工具协议、证据与引用规则 | “导师”“研究员”等角色人格，或用户可调的语气与详略偏好 |
| Assistant Profile | Product settings | 助手名称、身份、语气、默认详略程度与稳定行为偏好 | 用户事实、凭据、权限提升或绕过 Core Policy 的指令 |
| Task Overlay | 发起具体任务的产品能力 | 本次任务的输出格式、评分协议和临时约束 | 永久改写 Assistant Profile，或覆盖 Core Policy |

`llm-harness-runtime` 接收产品组装后的提示词并负责执行，不拥有 Folumi 的角色人格。产品代码中的通用 Chat prompt 必须保持身份中立；旧的 `You are a knowledgeable tutor` 属于收缩前 Tutor 产品形态的遗留身份，应移除。默认的 Folumi 身份应作为可见、可编辑的 Assistant Profile 初始值提供。

普通会话在创建时将当时的 Assistant Profile 快照到 Session 产品元数据，确保既有会话不会因之后修改全局配置而静默改变身份。所有 Chat 入口应复用同一个 Assistant Profile 组装函数，禁止在 Benchmark、临时会话或其他能力中复制另一套角色提示词。

Assistant Profile 的名称和身份说明分别参与组装。身份说明为空时，产品继续采用默认 Folumi 身份说明；配置界面必须展示这一回退结果，并在自定义名称与默认 Folumi 身份同时生效时明确提醒可能的身份冲突。界面还必须说明 Profile 修改只影响新会话，已有会话继续使用创建时保存的快照。

Benchmark 可以在 Assistant Profile 之上叠加“只输出答案实体”等评分约束，但该约束属于 Task Overlay，不应写回日常角色配置。每份端到端结果至少记录角色名称、角色说明内容哈希、角色来源和 Benchmark prompt revision；只有角色哈希、模型、评测 prompt 和数据版本一致的结果才适合直接比较。为避免把用户私有角色说明写入可提交结果，默认只保存哈希；仅在显式启用本地正文诊断时保存原文。

## 5. 保存的记忆模型

### 5.1 条目字段

每条保存的记忆是结构化产品实体：

| 字段 | 用途 |
| --- | --- |
| `id` | 稳定且不透明的条目标识 |
| `kind` | `fact`、`preference`、`goal` 或 `continuity` |
| `content` | 面向用户展示的简洁正文 |
| `topic_key` | 可选；在全局记忆集合中识别同一主题、处理冲突的稳定键 |
| `status` | `active`、`resolved` 或 `superseded` |
| `priority` | `normal` 或 `pinned` |
| `origin` | `user_explicit` 或 `assistant_suggested` |
| `source_refs` | 可选；指向支持本条记忆的 Session turn 或产品对象 |
| `provenance` | 有边界、机器可读的来源和确认元数据 |
| `idempotency_key` | runtime 调用重试时防止重复写入 |
| `created_at` / `updated_at` | 生命周期时间戳 |
| `last_confirmed_at` | 用户最近一次确认该内容仍有效的时间 |
| `valid_until` | 可选；超过该时间后不再召回 |
| `resolved_at` | 可选；目标或连续性事项完成的时间 |
| `revision` | 每次变更生成的精确 CAS 令牌 |

首版不保存模型生成的数字 `confidence`。可信度主要来自“用户是否明确表达或确认”“来源是否仍存在”“内容是否过期或被替代”，避免把不可校准的模型分数伪装成事实。

### 5.2 记忆类型

- `fact`：关于用户或其长期环境的稳定事实，例如称呼、职业背景或长期约束；
- `preference`：稳定的沟通、学习、格式或工作方式偏好；
- `goal`：用户正在推进、需要跨会话延续的中长期目标；
- `continuity`：未完成事项、后续约定或需要在之后继续处理的上下文，带完成状态和可选期限。

旧提案中的 `commitment` 与 `open_loop` 合并为 `continuity`，用状态表达是否完成；`strategy` 归入 Assistant Profile。类型只用于组织、过滤和检索，不决定物理存储位置。

## 6. 全局记忆与 Session 边界

Saved Memory 是用户级全局长期条目，可用于所有开启 Memory 的非临时会话。产品模型、数据库、API 和界面不提供 `workspace`、`scope_type` 或 `scope_id`，也不把 Knowledge Base、Notebook、导航页面或 Session ID 变成隐式记忆分区。

runtime Session 负责会话内连续性。Session turn 可以作为一条 Saved Memory 的来源引用；用户开启 History Recall 后，也可以按权限从旧 Sessions 中检索原始对话。但 Session 不是 Saved Memory 作用域：把同一内容再复制成“session-scoped memory”会制造两份权威来源，并与 runtime 的 compaction、删除和生命周期规则冲突。

如果以后出现真正需要跨多个 Sessions 隔离长期记忆的产品场景，必须先重新定义用户模型、生命周期与删除语义，而不是预留字段或直接拿 Session ID 代替。该能力不属于当前路线图。

## 7. 写入、冲突与失效规则

### 7.1 允许的写入路径

保存的记忆只通过以下路径产生：

1. 用户明确说“记住……”；
2. 用户直接陈述姓名、语言/回答偏好、无障碍需求、稳定目标或持续事项等明确、持久且有后续价值的信息，助手提出具体写入；
3. 用户在记忆页面手动新增。

助手不得从模糊暗示中推断用户事实，也不得主动保存一次性任务细节、第三方信息、凭据/秘密，或敏感的财务、健康、法律、证件号码和精确位置数据；除非用户明确要求，否则不写入这些内容。不确定持久性或敏感性时必须询问。

默认情况下，助手建议在确认前只是当前运行中的待决 mutation，不是可召回记忆，也不进入正式条目表。用户可以开启“允许助手无需审批添加记忆”，此时仅助手发起的 `memory_write` 可以在 runtime mutation gate 验证后直接落库；`memory_forget`、冲突解决、修改和彻底删除不继承该授权，仍需明确意图及相应确认。所有路径继续经过 `llm-harness-runtime-memory` 的访问控制、幂等策略、精确 revision 和实时 mutation gate。若 runtime 缺少批量 CAS 事务，产品不得用循环单条写入伪装原子操作。

### 7.2 `topic_key`

`topic_key` 用于表示“这些条目在讨论同一件事”，例如 `preferred_response_language` 或 `project_alpha_deadline`。它不是模型随意展示给用户的标签：

- 系统优先使用产品预定义键或从被修改条目继承；
- 助手提出新键时，必须和候选正文一起进入确认请求；
- 无法可靠确定主题时可以留空，此时系统只做幂等去重，不自动判定语义冲突；
- 用户可以在冲突处理界面选择“替代旧条目”或“作为不同主题保留”。

### 7.3 同一主题的写入决策

同一 `topic_key` 出现新内容时，在一个事务中执行：

- **内容等价**：不新建重复条目，只更新 `last_confirmed_at`、来源和 revision；
- **内容明确矛盾**：新条目成为 `active`，旧条目标记为 `superseded`，并通过关系记录 `supersedes`；确认界面必须同时展示新旧内容；
- **内容只是补充**：更新现有条目或作为独立主题保存，不得未经确认丢失旧信息；
- **关系不确定**：不写入，要求用户选择替代、合并或并存。

`topic_key` 为空时系统只做正文幂等去重，不推断语义冲突；用户仍可在界面中手动编辑或遗忘条目。

### 7.4 生命周期

- `active` 条目可以参与召回；
- `goal` 或 `continuity` 完成后变为 `resolved` 并记录 `resolved_at`，默认不参与召回，但仍可在界面查看；
- 被新事实替代的条目变为 `superseded`，默认不参与召回；
- `valid_until` 已过的条目计算为“已过期”，不参与召回，并在界面提示用户确认、修改或遗忘；
- 用户重新确认已过期条目时更新 `last_confirmed_at` 和 `valid_until`，同时生成新 revision；
- 用户“遗忘”时必须移除正文、来源和全文索引内容。若为幂等和审计保留最小 tombstone，只能包含 `id`、删除时间和不可逆内容摘要，不能保留可恢复正文；
- 历史版本只服务于并发检查和变更解释。遗忘必须同步清除历史表中的正文，不能以“版本历史”为由继续保存用户要求删除的内容。

### 7.5 并发和原子性

- 修改、完成、替代和遗忘都必须提交当前 `revision`；
- revision 不匹配时失败并返回当前版本，不自动覆盖；
- “新增条目并 supersede 旧条目”必须是同一 SQLite 事务；
- `idempotency_key` 在同一写入策略域内唯一，重试返回原结果；
- 冲突判定结果、用户确认内容和最终写入内容必须一致，不能在确认后再次由模型改写。

## 8. 历史检索

### 8.1 定义

历史检索是在用户明确开启后，对既有 runtime Sessions 做跨会话只读搜索。它解决“我们上次讨论到哪里”“之前决定了什么”之类的问题，但不会把每次对话变成长期记忆。

runtime Session 始终是历史正文的唯一权威来源。Folumi 可以维护可重建的本地检索索引和 runtime Session ID 映射，但不能复制一套平行会话仓库、重新实现 compaction，或把检索摘要当作权威历史。

### 8.2 控制规则

- 历史检索为独立开关，首版默认关闭；Memory 总开关关闭时它也必须关闭；
- 新建会话可选择“临时对话”。临时对话不读取保存的记忆、不检索旧会话、也不产生可供未来历史检索使用的索引；
- 删除某个 Session 后，对应历史索引必须同步删除；索引损坏或丢失时可以从仍存在且允许检索的 runtime Sessions 重建；
- 关闭历史检索不删除 Sessions，只停止新会话使用并暂停相应索引更新；用户可以另行选择清空派生索引；再次开启时必须补建缺失投影；
- 历史检索结果必须携带 Session、turn、时间和可打开来源，用户询问“你为什么这样说”时能够定位原对话；
- 历史片段不能自动写入 Saved Memory。用户可以基于片段明确发起“记住”，之后仍走正常确认和冲突流程。

### 8.3 检索流程

```text
当前问题
  -> 权限与临时会话检查
  -> Session 权限、生命周期和时间过滤
  -> 候选检索（首版 FTS；必要时再评估向量检索）
  -> 相关性、时间和多样性重排
  -> 严格的片段数与 token 预算
  -> 精确读取 runtime Session turn
  -> 将来源明确的历史片段交给本轮上下文构建
```

搜索摘要只能帮助选择候选，不能替代精确读取。默认排除当前 Session、临时会话、已删除会话和当前用户无权访问的会话。排序应综合文本相关性、最近时间和主题多样性，避免一段很长的旧会话占满上下文。

第一版先使用 SQLite FTS5。2026-08-04 的真实体验测试已经证明，`“我叫 <name>”` 与后续 `“我是谁”` 之间缺少词面重合时，模型生成的身份类搜索词可能返回 0 条；固定离线集此前没有覆盖这种意图改写。该问题优先通过 Saved Memory 主动保存稳定身份信息解决，同时补充 runtime 语义/混合检索能力评估；产品不得为此建立平行 Session 搜索链路。启用远程 Embedding 前必须说明哪些历史文本会发送给服务商。

### 8.4 runtime 依赖

历史检索需要 runtime 提供或明确认可以下边界：

- 按受控条件枚举 Session；
- 对 Session turn 建立可重建检索投影；
- 根据精确 Session/turn 引用读取正文；
- 删除、归档和临时会话状态能够驱动索引失效；
- 检索片段通过 runtime 上下文构建进入本轮，而不是由产品拼接平行 prompt。

当前 `SessionRepo` 具备创建、读取和列表能力，但没有一等跨 Session 搜索与检索投影契约。这个框架缺口必须先记录在 `docs/framework-feedback.md`，实现阶段优先扩展/复用 runtime，不在 Folumi 内建立第二套会话系统。

## 9. 召回策略

一次普通回答按以下顺序处理连续性信息：

1. 当前 runtime Session 的直接上下文；
2. 相关的全局 Saved Memory；
3. 用户开启后，按需进行 History Recall；
4. 用户明确选择的 Knowledge Base Sources 和 Notebook Notes。

这不是把四类正文全部塞进 prompt。每个来源都先经过权限、生命周期、状态和预算过滤：

- Saved Memory 只召回 `active`、未过期条目；`pinned` 提高排序但不绕过相关性；
- History Recall 只在当前问题需要跨会话连续性时调用；
- “我呢”“我叫什么”“你还记得吗”“我早上吃了什么”等直接或间接依赖记忆的问题属于强制按需召回；在回答“不知道”“你没告诉过我”或“记不起来”之前，助手必须先检索适用的记忆来源；这仍是当前 turn 内可见的工具调用，不是每轮预取或隐藏注入；
- 稳定的用户身份、偏好、目标和持续事项优先检索 Saved Memory；一次性事件与历史对话细节优先检索 History Recall；无法可靠判断来源时进行授权范围内的联合搜索；
- 检索词应由内容关键词构成。首次无结果但问题仍依赖记忆时，允许使用更简单的关键词、同义词或联合来源重试一次；重试后仍无结果才可明确表达不知道或不确定；
- 搜索命中后必须精确读取正文和 revision/turn；
- 每类来源都有独立数量与 token 上限，总预算由 runtime 上下文构建统一控制；
- 个性化记忆可以自然使用，不强制在每个回答中展示引用；涉及“你曾经说过/决定过”的历史陈述应提供可打开的会话来源；
- 助手应作为一个连贯的个体交流，而不是工具包装器。所有日常内部操作默认直接执行并直接回答，不播报工具名、工具选择、搜索或读取步骤；只有任务确实较长、需要用户同意、操作失败或留下重要不确定性、或用户询问过程时，才提供简短过程说明；
- Saved Memory 和 History Recall 默认静默使用。助手不得说“我查一下记忆”“我搜索一下历史”等工具自述，也不得重复界面已经展示的工具轨迹；取得结果后应以符合 Assistant Profile 的自然第一人称直接回答。只有检索失败或不确定性会实质影响答案、或用户主动询问来源/过程时，才说明检索情况；
- Memory 和历史对话不是外部事实证据，不能弱化 Knowledge Base 的引用要求。

长期记忆质量采用两层评测：runtime Session Recall 的证据检索层与完整 Agent 的最终回答层。LoCoMo 作为外部长对话困难集，不替代临时会话、删除失效、Saved Memory 审批/冲突、中文改写和自然表达等产品自有门禁。数据不随产品仓库分发，具体协议和复现方法见 `docs/qa/locomo-benchmark.md`。

助手主动保存 Saved Memory 暂不纳入 LoCoMo 分数。LoCoMo 没有按 Folumi 的持久性、安全、第三方信息、审批和冲突规则标注“这一轮应该形成什么记忆条目”；把全部对话事实当成应保存内容会反向鼓励过度记忆。在建立带 gold mutation、负例和跨 Session 生命周期验证的产品自有数据集前，主动保存只保留权限、审批、幂等、冲突和安全边界测试，不以临时拼出的单一 benchmark 分数宣称质量。

## 10. 存储设计

新系统从第一版直接使用 SQLite，不先建设 `items.json`。冲突关系、CAS、过期、全文检索和彻底遗忘会很快让单文档原子替换变得脆弱；SQLite 更符合本地优先、可事务化和可重建索引的要求。

建议表结构：

```text
memory_items
  id, kind, content, topic_key,
  status, priority, origin, created_at, updated_at,
  last_confirmed_at, valid_until, resolved_at, revision

memory_sources
  memory_id, source_type, source_id, source_revision, metadata

memory_relations
  from_id, relation_type, to_id
  # 首版至少支持 supersedes

memory_history
  memory_id, revision, operation, prior_value, changed_at, origin

memory_idempotency
  policy_scope, idempotency_key, result_id, result_revision, created_at

memory_items_fts
  memory_id, content, topic_key

session_recall_projection
  session_id, turn_id, occurred_at, content_hash

session_recall_fts
  session_id, turn_id, token postings
  # 使用 contentless FTS，只保存可重建词项，不复制可读取的 turn 正文
```

存储层必须提供：

- schema version 和前向迁移；不导入旧 L1/L2/L3 文件；
- 单事务 CAS 写入、替代、完成和遗忘；
- 稳定且不透明的 ID；
- 对类型、内容长度、来源、时间和状态转换进行失败即关闭的校验；
- FTS 索引与权威表在同一事务中更新；
- 删除/遗忘后的索引清理和可验证数据擦除；
- 数据库备份与恢复，但不能借备份功能静默恢复用户已经遗忘的正文。

## 11. 界面与 API

### 11.1 个性化页面

页面使用两个页级选项卡，并将实现术语映射为用户目的：

1. **关于我**（默认）：包含“使用个人信息”总开关、“允许助手自动补充”授权、个人信息列表、类型/状态筛选、来源、编辑、置顶、完成、永久移除，以及“参考过往对话”开关与隐私说明；
2. **助手设定**：助手名称和行为说明，不受“关于我”中的开关影响，对新会话生效。

个人信息列表默认展示有效条目，可切换查看已完成、已替代和已过期条目。冲突替代时必须并排展示新旧内容和来源。界面不出现 Saved Memory、History Recall、L1/L2/L3、Markdown 文件、物理路径或“整理记忆”任务；这些实现术语仅保留在 API、代码和技术文档中。

### 11.2 建议 API

Saved Memory：

- `GET /api/memory/items`：按类型和状态列出条目；
- `POST /api/memory/items`：用户在界面中明确新增；
- `PATCH /api/memory/items/:id`：使用 revision 修改、置顶、完成或重新确认；
- `DELETE /api/memory/items/:id`：使用 revision 遗忘并清除正文；
- `POST /api/memory/items/:id/resolve-conflict`：以一个事务完成替代、合并或并存决策。
- `GET/PATCH /api/memory/settings`：管理总开关、历史检索和助手新增免审批授权；关闭 Memory 时同时撤销后两项运行权限。

History Recall：

- `GET /api/memory/history/settings`：读取历史检索开关和索引状态；
- `PATCH /api/memory/history/settings`：开启、关闭或清空派生索引；
- `GET /api/memory/history/search`：供用户界面检查历史召回结果；Agent 路径仍使用受控 runtime 边界；
- `POST /api/sessions` 创建会话时接受明确的 `temporary` 标志。

API 只是产品界面边界。Agent 侧继续使用 runtime Knowledge/Memory/Session 能力，不增加同义的第二套模型工具。

## 12. 开关语义

- **Memory 总开关关闭**：新会话不挂载 Saved Memory 的读取/写入能力，也不运行 History Recall；已有数据不删除；Assistant Profile 仍正常生效；
- **助手新增免审批关闭（默认）**：助手可以主动提出值得保存的持久信息，但每次 `memory_write` 都需要用户确认；
- **助手新增免审批开启**：助手可以对符合安全边界的直接陈述执行 `memory_write`，不逐次询问；遗忘、冲突和其他破坏性变更仍需确认；
- **History Recall 关闭**：Saved Memory 仍可用，但不跨 Session 搜索；
- **临时对话开启**：本会话不读取或写入 Saved Memory、不检索历史，也不进入未来历史索引；
- **单条记忆失效/完成**：只影响该条目，不关闭整个系统；
- 所有权限在新 run 组装时重新计算，恢复旧 Session 不得沿用更宽的历史权限。

## 13. 非目标

- 恢复 L1/L2/L3 或任何隐藏的多层记忆；
- 根据 Notebook、知识库操作或所有聊天在后台批量建立用户画像；
- 在首版运行后台自动提取、总结和合并；
- 把 Session 正文复制进 Memory store；
- 用 Memory 替代知识库证据、Notebook 或 Assistant Profile；
- 在没有评测数据前引入图记忆、向量数据库或复杂衰减分数；
- 兼容、迁移或重新读取已经退役的旧记忆文件与维护 API；
- 为历史检索在产品仓库中重建 runtime Session、compaction 或上下文拼装系统。
- 在首版为每个 run 隐式执行历史搜索或自动注入旧对话片段。

## 14. 分阶段实施

### Phase 1：Saved Memory 基线

- [x] SQLite schema、FTS5、CAS、幂等与数据擦除测试；
- [x] Saved Memory 固定为用户级全局长期条目，不暴露 scope 字段；
- [x] `fact`、`preference`、`goal`、`continuity` 四种类型；
- [x] 显式新增、编辑、置顶、完成、重新确认和遗忘；
- [x] `topic_key`、冲突确认、原子 supersede、过期过滤；
- [x] runtime Memory/Knowledge 适配与 mutation gate；
- [x] Memory 页面和来源检查，不启用自动提取。
- [x] 有效期编辑、显式重新确认，以及排除内部历史和已遗忘正文的 JSON 导出。
- [x] 助手主动识别用户直接陈述的持久信息；新增免审批授权默认关闭且仅放行 `memory_write`，遗忘仍需审批。

实现说明：Saved Memory 默认关闭。开启后，新 run 才挂载 runtime 官方
Knowledge/Memory 能力；助手可以对用户直接陈述的姓名、稳定偏好和长期目标等提出
`memory_write`。默认仍需逐次确认；用户开启新增免审批授权后可以直接写入，但遗忘与
其他破坏性操作不在授权范围内。产品 API 的直接编辑仍使用 revision/CAS 与冲突规则。
Memory 页面还提供有效期修改、显式重新确认和 JSON 导出；导出不包含内部历史、tombstone、
策略密钥或已遗忘正文，也不构成旧文件迁移/导入通道。

### Phase 2：History Recall

状态：已完成。runtime 的跨 Session 搜索与精确 turn 投影契约已在
[llm-harness-runtime#104](https://github.com/oh-my-harness/llm-harness-runtime/issues/104)
及已合并的 [PR #105](https://github.com/oh-my-harness/llm-harness-runtime/pull/105)
中形成，Folumi 已固定到合并提交 `66f983d` 并完成下游接入和边界验证。

- [x] 确认并接入 runtime 跨 Session 搜索、可重建投影和精确 turn 读取边界；
- [x] 历史检索独立开关，默认关闭；Memory 总开关关闭时不运行；
- [x] 临时对话；创建时使用 runtime `SessionDurability::Temporary`，不挂载 Saved Memory 或 History Recall，也不进入派生索引；
- [x] 持久本地 SQLite/FTS5 候选索引；启动时按 Session revision 增量对账，索引可随时从权威 Session 重建；
- [x] runtime observer 驱动的 Session 更新和删除失效；
- [x] 模型主动执行 `knowledge_read` 时提供精确会话/turn 来源跳转；
- [x] 首版不挂载 `HistoryRecallPlugin`；历史搜索和读取只通过可见的 Agent 工具按需发生；
- [x] 建立离线评测集，记录相关率、错误召回率、延迟和上下文占用；Release 基线与复现方法见 [`docs/qa/history-recall-acceptance.md`](../qa/history-recall-acceptance.md)；
- [x] 不把历史片段自动提升为 Saved Memory。

当前集成状态记录在 `docs/framework-feedback.md`：History Recall 使用独立的
`SessionRecallAccessContext`，不会再被 Knowledge Base 选择切分；runtime 虽提供自动注入插件，
但 Folumi 首版明确不安装它。产品不会另建 Session 正文库或上下文拼装链路。
固定数据集上的 FTS5 词面基线已通过，但实际身份问答暴露了意图改写召回缺口。后续必须
扩充同义改写与更大规模数据，并在 runtime 边界内评估混合或语义检索；Folumi 不建设
平行 Session 搜索实现。

### Phase 3：受控增强

- 助手主动写入的安全边界、默认审批与可选免审批授权已实现；继续评估误记忆率和撤销体验；
- 评测 FTS 后再决定是否增加本地或远程 Embedding；
- 只有评测证明必要时才考虑后台候选生成、衰减或更复杂关系；
- 任何自动化默认关闭，并需要新的隐私、撤销和质量验收。

## 15. 验收标准

- 活动生产代码中不存在 L1/L2/L3 事件记录或整理工作流；
- Notebook、知识库操作不会自动创建 Saved Memory；普通聊天只允许对用户直接陈述且符合安全边界的持久信息提出写入；
- 保存的记忆可查看、修改、置顶、完成、重新确认和彻底遗忘；
- 同一主题的明确冲突通过单事务替代，旧条目不再被召回；
- 已完成、已替代、已过期和已删除条目不会进入正常召回；
- revision 不匹配时不覆盖，确认内容和最终 mutation 完全一致；
- History Recall 默认关闭，开启后只检索允许的 runtime Sessions，并能定位到精确来源；
- 删除 Session 或开启临时对话不会留下可检索的历史正文；
- History Recall 不自动创建 Saved Memory；
- Saved Memory 没有 workspace 或 Session scope；Session 内容由 runtime 管理，仅在明确确认后形成独立的全局条目；
- 关闭 Memory 后新 run 不挂载记忆和历史能力，但 Assistant Profile 与已有数据保持不变；
- Memory、历史和 Sources 各有独立预算，Memory 不会弱化知识引用要求；
- 旧分层文件不会影响新系统，也不会被应用静默迁移或删除；
- PRD、README、使用手册、API 文档和回归测试使用一致术语与开关语义。

## 16. 变更控制

以下变化必须先形成新的明确产品决策，并在同一次变更中同步更新 PRD、用户文档、隐私说明和回归测试：

- 默认开启历史检索或助手新增免审批授权；
- 在用户未开启新增免审批授权时，未经确认把候选内容写入 Saved Memory；
- 新增隐藏层级、后台整理、图记忆或复杂自动合并；
- 新增长期记忆分区、项目级记忆或 Session scope；
- 重新读取、迁移或导入旧 L1/L2/L3 文件；
- 将 Session 正文、Notebook 或 Knowledge Base 内容静默复制到 Memory store。
