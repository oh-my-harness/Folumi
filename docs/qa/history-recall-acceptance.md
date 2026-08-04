# History Recall 离线评测基线

> 日期：2026-08-04
>
> 结论：词面重合基线通过，但真实意图改写用例暴露召回缺口；不启用自动召回，下一步应在 runtime 边界内评估混合或语义检索。

## 1. 评测目标与边界

本评测验证 Folumi 已接入的 runtime History Recall 是否满足首版边界：

- 仅通过可见的 `knowledge_search` / `knowledge_read` 工具按需检索，不安装 run 前自动召回插件；
- runtime Session 是唯一权威来源，SQLite/FTS5 只是可重建的本地派生索引；
- 临时 Session 和已删除 Session 不可被检索；
- 检索片段有明确的上下文预算；
- 整个评测不调用模型、不访问网络、不读取用户数据，也不需要服务商凭据。

## 2. 环境与数据集

- 操作系统：Windows 11 家庭中文版，10.0.22631（Build 22631）；
- 处理器：Intel Core i7-12700H，14 核 / 20 逻辑处理器；
- Rust：`rustc 1.97.1 (8bab26f4f 2026-07-14)`，`x86_64-pc-windows-msvc`；
- runtime：`llm-harness-runtime` revision `66f983d`；
- 后端：本地 SQLite/FTS5；
- 数据：8 条固定正例（含英文和中文）、2 条不存在的普通负例、1 条临时 Session 负例、1 条删除 Session 负例。

固定数据集由测试在独立临时目录中构造，因此不会依赖真实用户历史，结果可重复。每次搜索最多返回 3 条，每条片段的产品上限为 1024 字节。

## 3. 指标定义

| 指标 | 定义 | 验收阈值 |
|---|---|---:|
| Recall@1 | 正确 Session 位于第一条结果的正例比例 | ≥ 87.5% |
| Recall@3 | 正确 Session 位于前三条结果的正例比例 | 100% |
| 错误 Top-1 率 | 正例中第一条不是正确 Session 的比例 | ≤ 12.5% |
| 负例误召回率 | 不存在、临时或已删除内容仍产生结果的比例 | 0% |
| 温热检索 P95 | 3 次预热后，200 次串行本地搜索的 P95 | ≤ 50 ms |
| 上下文占用 P95 | P95 返回片段字节数 / `3 × 1024` 字节预算 | ≤ 100% |

“约合 token”只用于工程量级观察：ASCII 字符按 4 字符/token，非 ASCII 字符按 1.2 token/字符估算；它不是特定服务商 tokenizer 的精确计数，也不作为硬性门禁。

## 4. Release 基线结果

| 指标 | 结果 |
|---|---:|
| Recall@1 | 100.0%（8/8） |
| Recall@3 | 100.0%（8/8） |
| 错误 Top-1 率 | 0.0% |
| 负例误召回率 | 0.0%（0/4） |
| 搜索 P50 | 2.261 ms |
| 搜索 P95 | 2.857 ms |
| 上下文 P95 | 175 字节，约 56 token |
| 上下文预算占用 P95 | 5.7% |

该结果来自 Release 模式的固定本地数据集。延迟数值只代表上述开发机上的小规模温热检索，不应外推为大数据集或其他存储设备的容量结论。

## 5. 复现方式

在仓库根目录执行：

```powershell
$env:CARGO_BUILD_JOBS='1'
cargo test -p tutor-web --lib --release history_recall_offline_quality_latency_and_context_baseline -- --ignored --nocapture
```

测试实现位于 `crates/tutor-web/src/history_recall_eval.rs`，默认标记为 ignored，避免普通单元测试把性能数字误当作稳定的跨机器基准。

## 6. 真实体验补充（2026-08-04）

用户在一个持久 Session 中直接陈述 `“我叫 <name>”`，随后在新 Session 询问 `“我是谁”`。Agent 正确调用了 `session_recall`，但生成的查询为“用户的身份 / 自我介绍 / 名字”一类意图词，与原句缺少足够词面重合，SQLite/FTS5 返回 0 条。Session、Recall 开关、索引 revision 和工具调用均正常，根因是当前词法检索无法稳定处理语义改写。

这说明原 8 条正例主要验证了关键词命中、生命周期、延迟和上下文预算，不能代表自然语言语义召回质量。稳定姓名等信息应优先进入 Saved Memory；History Recall 仍需加入同义改写、指代和长会话困难集。

## 7. 决策

当前 FTS5 仍满足词面检索、失效、延迟和上下文预算门槛，但已有实际证据表明它不足以单独承担自然语言意图改写。Folumi 不在产品仓库建立第二套 Session 检索；应先扩充困难集，并推动或复用 runtime 的混合/语义索引能力。若采用远程 Embedding，必须另行评估历史文本外发范围、用户开关、成本和可删除性。

LoCoMo 外部长对话困难集的适配方式、分层指标、许可边界和复现命令见 [`locomo-benchmark.md`](./locomo-benchmark.md)。原固定基线继续承担生命周期和性能门禁，LoCoMo 检索层负责暴露长历史、时间、多证据和对抗问题下的质量差距；两者不能互相替代。
