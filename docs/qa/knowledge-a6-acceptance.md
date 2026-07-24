# Runtime Knowledge A6 Acceptance Baseline

Date: 2026-07-24

Runtime revision:
`83bef164b36bd46ffa6f41cd6d3288a6b93cac4e`

This report closes the quality, security, and engineering measurement gate for
the course Knowledge migration. It measures the runtime boundary separately
from remote embedding and model latency so that the result is deterministic and
can be reproduced without credentials.

## Environment

- Windows 11 `10.0.22631`
- Intel Core i7-12700H, 14 cores / 20 logical processors
- Rust `1.97.1`, `x86_64-pc-windows-msvc`
- Optimized Cargo release profile
- LanceDB `0.30.0`
- Deterministic 32-dimensional hash embedding

## Retrieval Quality

The fixture contains five single-chunk course documents and five representative
queries. The test compares the existing product management search result with
the new runtime `KnowledgeSource` result. Both paths use the same LanceDB index,
so the comparison isolates the protocol adapter introduced by the migration.

| Query | Expected top document | Management path | Runtime path |
| --- | --- | --- | --- |
| `refund receipt thirty days` | `refund-policy.md` | top 1 | top 1 |
| `force mass acceleration` | `newton-laws.md` | top 1 | top 1 |
| `chlorophyll sunlight glucose` | `photosynthesis.md` | top 1 | top 1 |
| `SYN SYN-ACK ACK` | `tcp-handshake.md` | top 1 | top 1 |
| `derivative rate change tangent` | `calculus-derivative.md` | top 1 | top 1 |

All five expected documents ranked first. The complete top-three item ordering
was identical between the management path and runtime path for every query.

The comparison is enforced continuously by
`runtime_boundary_preserves_representative_management_search_quality`.

## Search and Read Latency

The release-mode benchmark performs five warm-up calls followed by 100 measured
calls. Values are warm local latency in milliseconds.

| Operation | P50 | P95 |
| --- | ---: | ---: |
| Management search | 4.600 ms | 5.159 ms |
| Runtime Knowledge search | 4.622 ms | 5.323 ms |
| Runtime Knowledge read | 3.513 ms | 3.797 ms |

Runtime search added 0.022 ms at P50 and 0.164 ms at P95 in this run. These
figures measure local protocol and storage overhead. A remote embedding provider
would add network and provider latency to both search paths.

Reproduce the measurement with:

```powershell
cargo test -p tutor-rag --release `
  knowledge_a6_search_and_read_latency_baseline -- `
  --ignored --nocapture
```

## Token and Durable Session Baseline

The Chat integration fixture executes `knowledge_search`, `knowledge_read`, and
a cited final answer through the runtime harness. It injects deterministic
provider-reported usage so the accounting and persistence path is stable:

| Field | Value |
| --- | ---: |
| Input tokens | 240 |
| Output tokens | 36 |
| Cache-read tokens | 12 |
| Cache-write tokens | 8 |
| Durable Session files | 5,572 bytes |
| Full read-body sentinel persisted | no |

The test scans the raw JSONL Session directory after the run. A unique sentinel
placed beyond the search snippet but inside the full read body does not appear
in `meta.json`, `entries.jsonl`, or any other Session file.

The machine had no configured LLM provider, common API-key environment
variable, Ollama, or LM Studio service. The token values above are therefore an
engineering baseline for the runtime usage pipeline, not a claim about a
particular production model. Provider-specific token and end-to-end latency
samples belong to release QA because they require a pinned external model and
credentials.

Reproduce the Session and usage baseline with:

```powershell
cargo test -p tutor-agent --test mock_integration `
  chat_uses_runtime_knowledge_tools_and_keeps_read_bodies_out_of_session -- `
  --nocapture
```

## Security and Diagnostics

The following automated evidence covers the remaining A6 security gate:

- missing trusted access fails closed;
- the model cannot supply a Knowledge Base authorization argument;
- forged source IDs are rejected;
- exact reads reject stale revisions and cross-KB item refs;
- a valid citation handle issued in one run is rejected in another run;
- full Knowledge read bodies are absent from raw durable Session files;
- public backend failures remain sanitized while controlled diagnostics retain
  the underlying detail.

The product integration coverage includes Chat, detailed Research, and
KB-backed Quiz. Runtime contract tests cover source search/read, authorization,
revision, and citation behavior.

## Conclusion

The migration preserves representative retrieval ordering, adds only a small
local runtime boundary overhead, keeps full evidence bodies out of durable
Session storage, and rejects cross-scope and cross-run trust escalation. The A6
engineering acceptance gate is complete. Manual testing against a particular
real model remains part of the general desktop release checklist rather than
the runtime protocol migration gate.
