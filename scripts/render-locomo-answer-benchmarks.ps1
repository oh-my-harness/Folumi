param(
    [string]$ResultsDirectory = (Join-Path (Split-Path $PSScriptRoot -Parent) 'benchmarks\locomo\answer-results'),
    [string]$OutputPath = (Join-Path (Split-Path $PSScriptRoot -Parent) 'benchmarks\locomo\charts\answer-comparison.svg')
)

$ErrorActionPreference = 'Stop'

function Escape-Xml([string]$Text) {
    return [System.Security.SecurityElement]::Escape($Text)
}

function Percent([object]$Value) {
    if ($null -eq $Value) { return 'n/a' }
    return [Math]::Round(([double]$Value) * 100, 1)
}

function Add-Grid(
    [System.Collections.Generic.List[string]]$Svg,
    [double]$Left,
    [double]$Top,
    [double]$Width,
    [double]$Height
) {
    foreach ($percent in 0, 25, 50, 75, 100) {
        $y = $Top + $Height - ($Height * $percent / 100)
        $Svg.Add("<line class='grid' x1='$Left' y1='$y' x2='$($Left + $Width)' y2='$y'/>")
        $Svg.Add("<text class='axis' x='$($Left - 12)' y='$($y + 4)' text-anchor='end'>$percent%</text>")
    }
}

if (-not (Test-Path -LiteralPath $ResultsDirectory -PathType Container)) {
    throw "LoCoMo answer results directory does not exist: $ResultsDirectory"
}

$runs = @(
    Get-ChildItem -LiteralPath $ResultsDirectory -Filter '*.json' -File |
        ForEach-Object {
            $report = Get-Content -LiteralPath $_.FullName -Raw -Encoding UTF8 | ConvertFrom-Json
            if ($report.schema_version -ne 1 -or $report.benchmark -ne 'locomo_agent_answer_accuracy') {
                throw "Unsupported LoCoMo answer result schema: $($_.FullName)"
            }
            $report
        } |
        Sort-Object generated_at, run_id
)

if ($runs.Count -eq 0) {
    throw "No LoCoMo Agent answer JSON results found in $ResultsDirectory"
}

$width = 1200
$height = 790
$left = 80
$plotWidth = 1060
$topPlotTop = 86
$topPlotHeight = 245
$bottomPlotTop = 470
$bottomPlotHeight = 210
$metricSeries = @(
    @{ Key = 'answer_f1'; Label = 'LoCoMo answer score'; Color = '#7c3aed' },
    @{ Key = 'exact_match'; Label = 'Exact match'; Color = '#2563eb' },
    @{ Key = 'abstention_accuracy'; Label = 'Category 5 abstention'; Color = '#d97706' },
    @{ Key = 'search_rate'; Label = 'History search rate'; Color = '#0f766e' }
)

$svg = [System.Collections.Generic.List[string]]::new()
$svg.Add("<svg xmlns='http://www.w3.org/2000/svg' width='$width' height='$height' viewBox='0 0 $width $height' role='img' aria-labelledby='title desc'>")
$svg.Add("<title id='title'>LoCoMo Agent answer benchmark comparison</title>")
$svg.Add("<desc id='desc'>End-to-end answer and tool-use metrics across saved runs, followed by category answer scores for the latest run.</desc>")
$svg.Add("<style>text{font-family:Inter,Segoe UI,Arial,sans-serif;fill:#172033}.title{font-size:22px;font-weight:600}.subtitle{font-size:14px;fill:#526079}.axis{font-size:12px;fill:#64748b}.label{font-size:12px}.value{font-size:11px;font-weight:600}.grid{stroke:#dbe2ea;stroke-width:1}.baseline{stroke:#94a3b8;stroke-width:1.2}.legend{font-size:12px}.note{font-size:11px;fill:#64748b}</style>")
$svg.Add("<rect width='$width' height='$height' fill='#ffffff'/>")
$svg.Add("<text class='title' x='$left' y='34'>LoCoMo Agent answer - Overall comparison</text>")
$svg.Add("<text class='subtitle' x='$left' y='58'>Answer metrics: higher is better; search rate is behavioral context, not a quality score</text>")
Add-Grid $svg $left $topPlotTop $plotWidth $topPlotHeight
$svg.Add("<line class='baseline' x1='$left' y1='$($topPlotTop + $topPlotHeight)' x2='$($left + $plotWidth)' y2='$($topPlotTop + $topPlotHeight)'/>")

$groupWidth = $plotWidth / $runs.Count
$barGap = 4
$barWidth = [Math]::Min(42, [Math]::Max(10, (($groupWidth * 0.72) - ($barGap * 3)) / 4))
for ($runIndex = 0; $runIndex -lt $runs.Count; $runIndex++) {
    $run = $runs[$runIndex]
    $barsWidth = ($barWidth * 4) + ($barGap * 3)
    $groupLeft = $left + ($groupWidth * $runIndex) + (($groupWidth - $barsWidth) / 2)
    for ($metricIndex = 0; $metricIndex -lt $metricSeries.Count; $metricIndex++) {
        $metric = $metricSeries[$metricIndex]
        $rawValue = $run.overall.($metric.Key)
        $value = if ($null -eq $rawValue) { 0.0 } else { [double]$rawValue }
        $barHeight = $topPlotHeight * $value
        $x = $groupLeft + (($barWidth + $barGap) * $metricIndex)
        $y = $topPlotTop + $topPlotHeight - $barHeight
        $svg.Add("<rect x='$x' y='$y' width='$barWidth' height='$barHeight' rx='2' fill='$($metric.Color)'><title>$(Escape-Xml $run.run_id) - $($metric.Label): $(Percent $rawValue)%</title></rect>")
        if ($runs.Count -le 4) {
            $svg.Add("<text class='value' x='$($x + ($barWidth / 2))' y='$($y - 5)' text-anchor='middle'>$(Percent $rawValue)</text>")
        }
    }
    $center = $left + ($groupWidth * ($runIndex + 0.5))
    $svg.Add("<text class='label' x='$center' y='$($topPlotTop + $topPlotHeight + 23)' text-anchor='middle'>$(Escape-Xml $run.run_id)</text>")
    $svg.Add("<text class='note' x='$center' y='$($topPlotTop + $topPlotHeight + 40)' text-anchor='middle'>$(Escape-Xml $run.configuration.model) - $([Math]::Round([double]$run.diagnostics.answer_latency_p95_ms)) ms P95</text>")
}

$legendX = $left
foreach ($metric in $metricSeries) {
    $svg.Add("<rect x='$legendX' y='405' width='12' height='12' rx='2' fill='$($metric.Color)'/>")
    $svg.Add("<text class='legend' x='$($legendX + 18)' y='415'>$($metric.Label)</text>")
    $legendX += 245
}

$latest = $runs[-1]
$svg.Add("<text class='title' x='$left' y='454'>Latest run by category - $(Escape-Xml $latest.run_id)</text>")
Add-Grid $svg $left $bottomPlotTop $plotWidth $bottomPlotHeight
$svg.Add("<line class='baseline' x1='$left' y1='$($bottomPlotTop + $bottomPlotHeight)' x2='$($left + $plotWidth)' y2='$($bottomPlotTop + $bottomPlotHeight)'/>")

$categories = @($latest.categories.PSObject.Properties | Sort-Object { [int]$_.Name })
$categoryWidth = $plotWidth / $categories.Count
$categoryBarWidth = [Math]::Min(54, $categoryWidth * 0.28)
for ($categoryIndex = 0; $categoryIndex -lt $categories.Count; $categoryIndex++) {
    $category = $categories[$categoryIndex]
    $center = $left + ($categoryWidth * ($categoryIndex + 0.5))
    $answer = [double]$category.Value.answer_f1
    $exact = [double]$category.Value.exact_match
    $answerHeight = $bottomPlotHeight * $answer
    $exactHeight = $bottomPlotHeight * $exact
    $answerX = $center - $categoryBarWidth - 3
    $exactX = $center + 3
    $svg.Add("<rect x='$answerX' y='$($bottomPlotTop + $bottomPlotHeight - $answerHeight)' width='$categoryBarWidth' height='$answerHeight' rx='2' fill='#7c3aed'><title>Category $($category.Name) - answer score: $(Percent $answer)%</title></rect>")
    $svg.Add("<rect x='$exactX' y='$($bottomPlotTop + $bottomPlotHeight - $exactHeight)' width='$categoryBarWidth' height='$exactHeight' rx='2' fill='#2563eb'><title>Category $($category.Name) - exact match: $(Percent $exact)%</title></rect>")
    $svg.Add("<text class='label' x='$center' y='$($bottomPlotTop + $bottomPlotHeight + 22)' text-anchor='middle'>Category $($category.Name)</text>")
}

$svg.Add("<rect x='$left' y='724' width='12' height='12' rx='2' fill='#7c3aed'/><text class='legend' x='$($left + 18)' y='734'>Answer score</text>")
$svg.Add("<rect x='$($left + 125)' y='724' width='12' height='12' rx='2' fill='#2563eb'/><text class='legend' x='$($left + 143)' y='734'>Exact match</text>")
$costDisplay = [Math]::Round([double]$latest.diagnostics.usage.cost_usd, 4)
$svg.Add("<text class='note' x='$left' y='764'>Latest: $($latest.dataset_counts.questions_scored) questions - cost USD $costDisplay - errors $($latest.overall.errors) - unexpected tool calls $($latest.overall.unexpected_tool_calls) - tool narrations $($latest.overall.tool_narrations)</text>")
$svg.Add("<text class='note' x='$left' y='782'>Provenance: Folumi $(Escape-Xml $latest.provenance.folumi_revision) - runtime $(Escape-Xml $latest.provenance.runtime_revision) - LoCoMo $(Escape-Xml $latest.provenance.dataset_revision)</text>")
$svg.Add('</svg>')

$parent = Split-Path $OutputPath -Parent
if ($parent -and -not (Test-Path -LiteralPath $parent)) {
    New-Item -ItemType Directory -Path $parent -Force | Out-Null
}
$utf8 = [System.Text.UTF8Encoding]::new($false)
[System.IO.File]::WriteAllText($OutputPath, ($svg -join [Environment]::NewLine), $utf8)
Write-Output $OutputPath
