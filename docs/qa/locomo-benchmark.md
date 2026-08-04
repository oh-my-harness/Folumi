# LoCoMo 长期记忆评测方案

> 日期：2026-08-04
>
> 状态：第一阶段（runtime Session Recall 离线检索评测）已接入；端到端回答质量评测待建立首个可复现基线。

## 1. 为什么引入 LoCoMo

[LoCoMo](https://github.com/snap-research/locomo) 是面向超长期对话记忆的公开评测集。当前官方子集包含 10 组长对话，每组跨多个带时间戳的 Session，并提供问题、答案、问题类别和证据对话编号。它能补足 Folumi 原有 8 条固定词面用例覆盖不到的能力：

- 跨 Session 找到具体事实；
- 时间推理；
- 多证据组合；
- 开放域单 Session 问答；
- 带干扰答案的对抗问题。

LoCoMo 不是完整的产品验收。它主要是英文合成长对话，只有 10 组会话，不能覆盖中文体验、用户审批、隐私开关、Saved Memory 写入准确性、删除失效或助手表达风格。因此 Folumi 保留自己的产品边界测试，并把 LoCoMo 作为外部困难集。

## 2. 许可和数据管理

LoCoMo 仓库采用 **CC BY-NC 4.0**。为避免把非商业数据混入 Folumi 源码和发布物：

- 仓库不提交、复制或重新分发 `locomo10.json`；
- 评测者从官方仓库自行取得数据，并通过环境变量提供本地路径；
- 公开评测结果时注明数据集和论文来源；
- 商业使用场景须单独确认许可，不能因为评测适配器是开源代码就推定数据可商用。

## 3. 分层评测

### 3.1 检索层（已实现）

适配器把每组 LoCoMo conversation 隔离成一个临时 Folumi 用户历史，把每个 LoCoMo Session 导入为一个持久 runtime Session。每条原始对话都走 runtime Session API，并由现有 Session Recall projector 和 SQLite/FTS5 派生索引处理；产品仓库没有建立另一套检索或会话存储。

每个问题最多取产品当前的 3 条检索结果，报告：

| 指标 | 含义 |
|---|---|
| Hit@1 / Hit@3 | 前 1 / 3 条结果中是否至少包含一条标注证据 |
| MRR@3 | 第一条相关证据的倒数排名 |
| Evidence Recall@3 | 所有标注证据中被前三条结果覆盖的比例 |
| 分类指标 | 按 LoCoMo category 1–5 分开报告，避免总分掩盖能力差异 |
| P50 / P95 延迟 | 包含真实长历史规模下的本地检索耗时 |
| Context P95 | 返回片段占用的字节数，继续受产品上下文上限约束 |

没有 evidence 的题目不参与检索正确率，只单独计数。适配器会修复可无歧义的纯格式问题（例如空格分隔的多个编号、`D:11:26` 和 `D30:05`），但不会猜测指向不存在 turn 的标注；后者作为 `annotation_issues` 报告并从该题的有效证据集合中排除。Category 5 的当前结果只说明能否检索到证据，不能代表模型是否抵抗了干扰答案。

### 3.2 端到端回答层（下一阶段）

端到端层应通过 Folumi 正常 Agent / runtime 链路提问，记录模型、服务商、prompt revision、工具轨迹和检索来源，再按 LoCoMo 官方分类规则计算回答分数。该层必须单独报告：

- 回答质量，而不是只看检索命中；
- Category 5 对抗题正确率；
- 工具调用率、无效检索率和未检索却猜测的比例；
- 模型与评审器版本，避免把 scorer 漂移误认为记忆能力变化；
- “过程旁白率”：不应出现“我查一下记忆”“我搜索一下历史”等工具自述。

端到端评测会产生模型费用，且结果受模型版本影响，所以不进入默认 CI。在固定模型和评审规则前，不设置拍脑袋的通过阈值。

## 4. 运行方式

先从官方仓库取得数据，例如将仓库克隆到本机，然后在 Folumi 根目录执行：

```powershell
$env:FOLUMI_LOCOMO_DATASET='C:\path\to\locomo\data\locomo10.json'
$env:CARGO_BUILD_JOBS='1'
cargo test -p tutor-web --lib --release locomo_history_recall_retrieval_benchmark -- --ignored --nocapture
```

快速验证适配器时可限制规模：

```powershell
$env:FOLUMI_LOCOMO_MAX_SAMPLES='1'
$env:FOLUMI_LOCOMO_MAX_QUESTIONS='20'
```

限制变量只用于 smoke test，正式可比结果必须清空限制并运行完整 10 组数据。测试只写入独立临时目录，不读取或修改真实用户数据，也不调用在线模型。

## 5. 基线管理

首次完整运行后，把以下信息作为一条带日期的基线记录：Folumi commit、runtime revision、LoCoMo commit、操作系统、CPU、所有总体/分类指标和运行参数。后续优化只有在同一数据、同一 Top-K 和同一评分定义下才可比较。

LoCoMo 不替代以下 Folumi 自有门禁：临时 Session 隔离、删除失效、Saved Memory 冲突和过期、写入审批、中文同义改写、用户名字/偏好召回，以及自然表达回归测试。

## 6. 首次诊断基线

2026-08-04 在 Folumi `704c3e5` 之后的工作树、runtime `66f983d`、LoCoMo `3eb6f2c` 上完成首次全量 Debug 运行。共读取 10 组 conversation、1986 道题；1982 道有可用 evidence 的题进入检索评分，4 道无 evidence。适配器报告 3 个 annotation issue，其中可用证据仍足以评分；没有题目因完全缺少有效证据被丢弃。

| 范围 | 题数 | Hit@1 | Hit@3 | MRR@3 | Evidence Recall@3 |
|---|---:|---:|---:|---:|---:|
| 总体 | 1982 | 30.6% | 45.5% | 37.0% | 32.9% |
| Category 1 | 282 | 13.8% | 27.0% | 19.2% | 10.4% |
| Category 2 | 321 | 37.4% | 52.3% | 43.9% | 45.6% |
| Category 3 | 92 | 9.8% | 18.5% | 13.8% | 8.2% |
| Category 4 | 841 | 34.2% | 50.2% | 41.1% | 47.8% |
| Category 5 | 446 | 33.6% | 49.1% | 40.2% | 47.8% |

本次 Debug 运行的检索 P50 为 9.474 ms、P95 为 12.731 ms，Context P95 为 659 字节。性能数字只用于确认量级和上下文边界，正式性能基线仍应使用 Release 模式。

这个结果不是验收通过：它表明当前 FTS5 词法召回在长历史中只能覆盖不足一半的问题，Category 1 的多证据聚合和 Category 3 的推理题尤其薄弱。下一轮 runtime 检索改进应以同一适配器复测，优先提升 Hit@3 和 Evidence Recall@3；在有第二个可比实现前暂不设置硬门槛。
