# LoCoMo 评测结果

本目录保存机器可读、只追加不覆盖的检索评测与 Agent 回答评测结果，以及由结果生成的对比图。LoCoMo 数据集采用 CC BY-NC 4.0 许可，本仓库不会复制或提交数据集正文。

## 一键运行控制台

在仓库根目录运行：

```powershell
.\scripts\locomo-benchmark.ps1
```

脚本会在本机打开一个 Benchmark 控制台。页面可以选择检索或端到端回答评测，配置 LoCoMo 数据集路径、Debug/Release、Sample 数、每组题目数、Run ID、模型和 API 服务，并实时显示 Cargo 日志、聚合指标、分类指标、历史结果和对比图。建议先用 `1` 个 Sample、`5` 道题做 smoke test，再逐步扩大范围；回答评测会产生真实的模型调用和费用。

也可以预先指定数据集、端口，或只启动服务而不自动打开浏览器：

```powershell
.\scripts\locomo-benchmark.ps1 -Dataset 'C:\path\to\locomo\data\locomo10.json' -Port 8765
.\scripts\locomo-benchmark.ps1 -NoBrowser
```

控制台只监听 `127.0.0.1`。页面中输入的 API Key 不写入浏览器存储、结果文件或日志，只存在于控制台进程内存及当前评测子进程环境；也可以在启动脚本前设置服务商对应的环境变量，让页面的 Key 输入框保持为空。关闭运行脚本的终端即可关闭控制台。

## 记录一次检索评测

运行检索评测时，应明确指定结果输出路径和版本来源：

```powershell
$env:FOLUMI_LOCOMO_DATASET='C:\path\to\locomo\data\locomo10.json'
$env:FOLUMI_LOCOMO_OUTPUT='benchmarks\locomo\results\2026-08-04-runtime-66f983d-fts5-debug.json'
$env:FOLUMI_BENCHMARK_RUN_ID='runtime-66f983d-fts5-debug'
$env:FOLUMI_BENCHMARK_FOLUMI_REVISION=(git rev-parse HEAD)
$env:FOLUMI_BENCHMARK_RUNTIME_REVISION='66f983d0a4c024c34e70bff3587cd4c44fb3b26f'
$env:FOLUMI_BENCHMARK_LOCOMO_REVISION='3eb6f2c585f5e1699204e3c3bdf7adc5c28cb376'
$env:CARGO_BUILD_JOBS='1'
cargo test -p tutor-web --lib locomo_history_recall_retrieval_benchmark -- --ignored --nocapture
```

正式记录延迟基线时请增加 `--release`。当检索设置和代码版本完全一致时，Debug 与 Release 的质量指标可以比较，但延迟指标不能直接比较。

每份结果都会保存原始命中数和证据数、比例指标、运行配置、按类别和 conversation 划分的结果、数据集计数、延迟以及代码版本来源。不要修改旧结果来代表新的实现；应使用新的 `run_id` 新增一份 JSON 文件。

## 重新生成检索对比图

```powershell
.\scripts\render-locomo-benchmarks.ps1
```

绘图脚本读取 `results/` 中所有 schema-v1 JSON 文件，按时间排序后生成 `charts/retrieval-comparison.svg`。上半部分比较各次运行的总体指标，下半部分展示最新一次运行的分类指标。

词法/向量混合检索、候选融合、时间过滤、相邻 turn 扩展、多样性控制和重排序等能力属于 `llm-harness-runtime-session-recall`。Folumi 只保留评测适配器、产品策略和回归基线，不在产品仓库中另建一套检索实现。

## 记录一次 Agent 回答评测

回答评测会对每道选中的问题发起一次在线模型请求，并运行真实的 Folumi Chat Agent 和 runtime History Recall。完整数据集约有 1,986 道题；正式全量运行前，应先执行小规模 smoke test 并检查费用：

```powershell
$env:FOLUMI_LOCOMO_DATASET='C:\path\to\locomo\data\locomo10.json'
$env:FOLUMI_LOCOMO_MAX_SAMPLES='1'
$env:FOLUMI_LOCOMO_MAX_QUESTIONS='5'
$env:FOLUMI_LOCOMO_ANSWER_OUTPUT='benchmarks\locomo\answer-results\2026-08-04-smoke.json'
$env:FOLUMI_BENCHMARK_RUN_ID='answer-smoke'
$env:FOLUMI_BENCHMARK_FOLUMI_REVISION=(git rev-parse HEAD)
$env:FOLUMI_BENCHMARK_RUNTIME_REVISION='66f983d0a4c024c34e70bff3587cd4c44fb3b26f'
$env:FOLUMI_BENCHMARK_LOCOMO_REVISION='3eb6f2c585f5e1699204e3c3bdf7adc5c28cb376'
$env:LLM_PROVIDER='anthropic' # 也可以使用 openai / deepseek
$env:LLM_MODEL='固定的模型 ID'
# 另行设置对应服务商的 API key 环境变量，禁止打印或提交密钥。
$env:CARGO_BUILD_JOBS='1'
cargo test -p tutor-web --lib locomo_agent_answer_accuracy_benchmark -- --ignored --nocapture
```

正式全量运行时，请清除 `FOLUMI_LOCOMO_MAX_SAMPLES` 和 `FOLUMI_LOCOMO_MAX_QUESTIONS`。每道题使用独立的临时答题 Session，确保先前的模型回答不会进入 History Recall 并污染后续问题。

默认结果不包含题目、标准答案和模型回答正文，避免提交聚合指标时重新分发数据集。只有在本地分析失败样本时才应设置 `FOLUMI_LOCOMO_INCLUDE_TEXT=true`，并且不得提交由此生成的结果文件。

Category 1–4 按照 LoCoMo 的分类规则计算 token F1。Category 5 使用自由回答形式的拒答文本（`No information available`），而不是论文运行脚本中的二选一展示，因此该类别的拒答准确率是 Folumi 产品指标，不能与论文分数直接横向比较。

保存至少一次运行结果后，可以生成回答质量对比图：

```powershell
.\scripts\render-locomo-answer-benchmarks.ps1
```

绘图脚本读取 `answer-results/` 并生成 `charts/answer-comparison.svg`。结果包含总体和分类回答分数、Exact Match、工具使用率、错误数、延迟、服务商返回的 token 用量与费用。
