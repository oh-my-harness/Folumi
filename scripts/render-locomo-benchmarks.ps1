param(
    [string]$ResultsDirectory = (Join-Path (Split-Path $PSScriptRoot -Parent) 'benchmarks\locomo\results'),
    [string]$OutputPath = (Join-Path (Split-Path $PSScriptRoot -Parent) 'benchmarks\locomo\charts\retrieval-comparison.svg')
)

$ErrorActionPreference = 'Stop'

function Escape-Xml([string]$Text) {
    return [System.Security.SecurityElement]::Escape($Text)
}

function Percent([double]$Value) {
    return [Math]::Round($Value * 100, 1)
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
    throw "LoCoMo results directory does not exist: $ResultsDirectory"
}

$runs = @(
    Get-ChildItem -LiteralPath $ResultsDirectory -Filter '*.json' -File |
        ForEach-Object {
            $report = Get-Content -LiteralPath $_.FullName -Raw -Encoding UTF8 | ConvertFrom-Json
            if ($report.schema_version -ne 1 -or $report.benchmark -ne 'locomo_history_recall_retrieval') {
                throw "Unsupported LoCoMo result schema: $($_.FullName)"
            }
            $report
        } |
        Sort-Object generated_at, run_id
)

if ($runs.Count -eq 0) {
    throw "No LoCoMo JSON results found in $ResultsDirectory"
}

$comparisonSearchLimit = [int]$runs[0].configuration.search_limit
foreach ($run in $runs) {
    if ([int]$run.configuration.search_limit -ne $comparisonSearchLimit) {
        throw "Cannot compare LoCoMo runs with different search_limit values"
    }
}

$width = 1200
$height = 760
$left = 80
$plotWidth = 1060
$topPlotTop = 80
$topPlotHeight = 245
$bottomPlotTop = 430
$bottomPlotHeight = 220
$metricSeries = @(
    @{ Key = 'hit_at_1'; Label = 'Hit@1'; Color = '#7c3aed' },
    @{ Key = 'hit_at_k'; Label = "Hit@$comparisonSearchLimit"; Color = '#2563eb' },
    @{ Key = 'mrr_at_k'; Label = "MRR@$comparisonSearchLimit"; Color = '#0f766e' },
    @{ Key = 'evidence_recall_at_k'; Label = "Evidence Recall@$comparisonSearchLimit"; Color = '#d97706' }
)

$svg = [System.Collections.Generic.List[string]]::new()
$svg.Add("<svg xmlns='http://www.w3.org/2000/svg' width='$width' height='$height' viewBox='0 0 $width $height' role='img' aria-labelledby='title desc'>")
$svg.Add("<title id='title'>LoCoMo History Recall benchmark comparison</title>")
$svg.Add("<desc id='desc'>Overall retrieval metrics across saved benchmark runs, followed by category metrics for the latest run.</desc>")
$svg.Add("<style>text{font-family:Inter,Segoe UI,Arial,sans-serif;fill:#172033}.title{font-size:22px;font-weight:600}.subtitle{font-size:14px;fill:#526079}.axis{font-size:12px;fill:#64748b}.label{font-size:12px}.value{font-size:11px;font-weight:600}.grid{stroke:#dbe2ea;stroke-width:1}.baseline{stroke:#94a3b8;stroke-width:1.2}.legend{font-size:12px}.note{font-size:11px;fill:#64748b}</style>")
$svg.Add("<rect width='$width' height='$height' fill='#ffffff'/>")
$svg.Add("<text class='title' x='$left' y='34'>LoCoMo History Recall - Overall comparison</text>")
$svg.Add("<text class='subtitle' x='$left' y='57'>Higher is better - every run remains separately versioned as JSON</text>")
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
        $value = [double]$run.overall.($metric.Key)
        $barHeight = $topPlotHeight * $value
        $x = $groupLeft + (($barWidth + $barGap) * $metricIndex)
        $y = $topPlotTop + $topPlotHeight - $barHeight
        $svg.Add("<rect x='$x' y='$y' width='$barWidth' height='$barHeight' rx='2' fill='$($metric.Color)'><title>$(Escape-Xml $run.run_id) - $($metric.Label): $(Percent $value)%</title></rect>")
        if ($runs.Count -le 4) {
            $svg.Add("<text class='value' x='$($x + ($barWidth / 2))' y='$($y - 5)' text-anchor='middle'>$(Percent $value)</text>")
        }
    }
    $svg.Add("<text class='label' x='$($left + ($groupWidth * ($runIndex + 0.5)))' y='$($topPlotTop + $topPlotHeight + 23)' text-anchor='middle'>$(Escape-Xml $run.run_id)</text>")
}

$legendX = $left
foreach ($metric in $metricSeries) {
    $svg.Add("<rect x='$legendX' y='374' width='12' height='12' rx='2' fill='$($metric.Color)'/>")
    $svg.Add("<text class='legend' x='$($legendX + 18)' y='384'>$($metric.Label)</text>")
    $legendX += 155
}

$latest = $runs[-1]
$searchLimit = $comparisonSearchLimit
$svg.Add("<text class='title' x='$left' y='414'>Latest run by category - $(Escape-Xml $latest.run_id)</text>")
Add-Grid $svg $left $bottomPlotTop $plotWidth $bottomPlotHeight
$svg.Add("<line class='baseline' x1='$left' y1='$($bottomPlotTop + $bottomPlotHeight)' x2='$($left + $plotWidth)' y2='$($bottomPlotTop + $bottomPlotHeight)'/>")

$categories = @($latest.categories.PSObject.Properties | Sort-Object { [int]$_.Name })
$categoryWidth = $plotWidth / $categories.Count
$categoryBarWidth = [Math]::Min(54, $categoryWidth * 0.28)
for ($categoryIndex = 0; $categoryIndex -lt $categories.Count; $categoryIndex++) {
    $category = $categories[$categoryIndex]
    $center = $left + ($categoryWidth * ($categoryIndex + 0.5))
    $hit = [double]$category.Value.hit_at_k
    $evidence = [double]$category.Value.evidence_recall_at_k
    $hitHeight = $bottomPlotHeight * $hit
    $evidenceHeight = $bottomPlotHeight * $evidence
    $hitX = $center - $categoryBarWidth - 3
    $evidenceX = $center + 3
    $svg.Add("<rect x='$hitX' y='$($bottomPlotTop + $bottomPlotHeight - $hitHeight)' width='$categoryBarWidth' height='$hitHeight' rx='2' fill='#2563eb'><title>Category $($category.Name) - Hit@${searchLimit}: $(Percent $hit)%</title></rect>")
    $svg.Add("<rect x='$evidenceX' y='$($bottomPlotTop + $bottomPlotHeight - $evidenceHeight)' width='$categoryBarWidth' height='$evidenceHeight' rx='2' fill='#d97706'><title>Category $($category.Name) - Evidence Recall@${searchLimit}: $(Percent $evidence)%</title></rect>")
    $svg.Add("<text class='label' x='$center' y='$($bottomPlotTop + $bottomPlotHeight + 22)' text-anchor='middle'>Category $($category.Name)</text>")
}

$svg.Add("<rect x='$left' y='700' width='12' height='12' rx='2' fill='#2563eb'/><text class='legend' x='$($left + 18)' y='710'>Hit@$searchLimit</text>")
$svg.Add("<rect x='$($left + 105)' y='700' width='12' height='12' rx='2' fill='#d97706'/><text class='legend' x='$($left + 123)' y='710'>Evidence Recall@$searchLimit</text>")
$svg.Add("<text class='note' x='$left' y='742'>Latest provenance: Folumi $(Escape-Xml $latest.provenance.folumi_revision) - runtime $(Escape-Xml $latest.provenance.runtime_revision) - LoCoMo $(Escape-Xml $latest.provenance.dataset_revision) - $($latest.profile)</text>")
$svg.Add('</svg>')

$parent = Split-Path $OutputPath -Parent
if ($parent -and -not (Test-Path -LiteralPath $parent)) {
    New-Item -ItemType Directory -Path $parent -Force | Out-Null
}
$utf8 = [System.Text.UTF8Encoding]::new($false)
[System.IO.File]::WriteAllText($OutputPath, ($svg -join [Environment]::NewLine), $utf8)
Write-Output $OutputPath
