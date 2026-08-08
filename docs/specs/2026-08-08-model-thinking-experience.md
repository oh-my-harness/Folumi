# 模型思考档位与展示设计

日期：2026-08-08

## 决策

Folumi 只在会话输入框提供思考强度入口，可选关闭、最少、低、中、高和极高；设置页不重复展示该控件。每个会话保存自己的档位，同时记住每个模型最近一次选择，作为该模型新会话的默认值；切换模型时恢复该模型上次使用的档位。该配置使用 runtime 的 `ThinkingLevel`，由 provider adapter 翻译为对应接口参数；不在产品层重新实现供应商协议。

模型接口实际返回可展示的 reasoning content 时，聊天界面在最终答案上方用弱化小字显示“思考过程”。生成期间保持展开并限制高度，完成后默认折叠。没有返回思考内容的模型不显示空入口。

## 数据边界

- 思考内容使用独立流事件，不拼接进最终答案正文。
- runtime Session 中的 `Thinking` block 是历史恢复的权威来源。
- 思考内容不进入个人信息、History Recall、RAG 索引或引用内容。
- 只展示 provider 明确返回的内容，不通过提示词要求模型伪造思维链。
- reasoning signature、加密思考块和 redacted thinking 不作为用户可见正文。

## 兼容策略

- Anthropic 使用 adapter 的 adaptive effort 或 token budget 映射。
- 标准 OpenAI-compatible 接口使用 `reasoning_effort`。
- DeepSeek Base URL 使用 thinking toggle 协议并解析 `reasoning_content`。
- 接口不支持所选档位时，应返回明确的 provider 错误；关闭档位保持最保守兼容行为。

## 交互

- 输入框工具栏是唯一入口，显示当前档位，点击后可直接切换六档；弹层只保留档位和“当前会话”提示。
- 生成过程中锁定档位，避免一次回答中途改变运行参数。
- 运行中：显示“思考中”，小字实时更新，区域最多约三行。
- 完成后：显示折叠的“思考过程”，用户点击后查看完整内容。
- 思考区与工具状态、资料引用和 Trace 分离。
- 历史会话重新打开后保持同一条回答与对应思考内容的关联。
