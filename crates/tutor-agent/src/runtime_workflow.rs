use llm_harness_runtime::workflow::model::{ConditionExpr, Edge, EdgeCondition, Step, Workflow};
use llm_harness_runtime::workflow::plan::validate_workflow;
use llm_harness_runtime_knowledge::{KNOWLEDGE_READ_TOOL_NAME, KNOWLEDGE_SEARCH_TOOL_NAME};
use llm_harness_types::SPAWN_AGENT_TOOL_NAME;

use crate::error::{Result, TutorError};

pub const QUIZ_GENERATION_WORKFLOW_ID: &str = "tutor.quiz_generation";
pub const MEMORY_WORKFLOW_ID: &str = "tutor.memory";
pub const RESEARCH_WORKFLOW_ID: &str = "tutor.research";

pub fn quiz_generation_workflow() -> Workflow {
    Workflow {
        entry_step: "collect_sources".into(),
        steps: vec![
            Step::executor(
                "collect_sources",
                "Collect quiz sources",
                "tutor.quiz.collect_sources",
                None,
            ),
            Step::llm(
                "generate_questions",
                "Generate grounded questions",
                "Read the workflow Context. The `quiz_generation_prompt` variable contains the full source-grounded quiz generation instruction. \
                 Generate grounded single-choice quiz questions from those sources. If prior step history includes verifier repair feedback, repair the draft. \
                 End the step with only the JSON object {\"questions\":[...]} using the exact question schema requested in Context. \
                 Do not wrap the JSON in Markdown fences or add prose before or after it.",
                vec![],
            )
            .with_structured(Some(true)),
            Step::llm(
                "verify_questions",
                "Verify generated questions",
                "Read the workflow Context and prior generate_questions structured result. Strictly verify every question against its cited source chunks. \
                 End the step with only {\"verdict\":\"pass\",\"issues\":[]} if all questions are grounded. \
                 If any question is unsupported, contradictory, or wrongly cited, end with only \
                 {\"verdict\":\"fail\",\"action\":\"repair\",\"issues\":[\"...\"]}. Do not use Markdown fences.",
                vec![],
            )
            .with_structured(Some(true)),
            Step::executor(
                "publish_questions",
                "Publish verified questions",
                "tutor.quiz.publish_questions",
                None,
            ),
        ],
        edges: vec![
            Edge {
                from: "collect_sources".into(),
                to: "generate_questions".into(),
                condition: None,
            },
            Edge {
                from: "generate_questions".into(),
                to: "verify_questions".into(),
                condition: None,
            },
            Edge {
                from: "verify_questions".into(),
                to: "publish_questions".into(),
                condition: Some(verdict_condition("pass")),
            },
            Edge {
                from: "verify_questions".into(),
                to: "generate_questions".into(),
                condition: Some(action_condition("repair")),
            },
        ],
    }
}

pub fn validate_quiz_generation_workflow() -> Result<()> {
    validate_workflow(&quiz_generation_workflow()).map_err(|err| {
        TutorError::Internal(format!(
            "runtime workflow validation failed for {QUIZ_GENERATION_WORKFLOW_ID}: {err}"
        ))
    })
}

pub fn memory_workflow() -> Workflow {
    memory_workflow_with_allowed_tools(vec![
        "list_memory_events".into(),
        "search_memory_events".into(),
        "read_memory_event".into(),
        "read_memory_context".into(),
        "read_memory_source".into(),
        "list_memory_entries".into(),
        "search_memory_entries".into(),
        "read_memory_entry".into(),
        "read_memory_entry_sources".into(),
    ])
}

pub fn memory_workflow_with_allowed_tools(allowed_tools: Vec<String>) -> Workflow {
    Workflow {
        entry_step: "prepare_memory".into(),
        steps: vec![
            Step::executor(
                "prepare_memory",
                "Prepare memory workflow input",
                "tutor.memory.prepare",
                None,
            ),
            Step::llm(
                "run_memory",
                "Run memory workflow",
                "Read the workflow Context. The `memory_prompt` variable contains the full memory maintenance instruction, including target file, action, current Markdown, normalized evidence, and output schema. \
                 Maintain learner memory according to that instruction. End the step with only the JSON object requested by `memory_prompt`; do not wrap it in Markdown fences or add prose.",
                allowed_tools,
            )
            .with_structured(Some(true)),
        ],
        edges: vec![Edge {
            from: "prepare_memory".into(),
            to: "run_memory".into(),
            condition: Some(prepared_condition()),
        }],
    }
}

pub fn validate_memory_workflow() -> Result<()> {
    validate_workflow(&memory_workflow()).map_err(|err| {
        TutorError::Internal(format!(
            "runtime workflow validation failed for {MEMORY_WORKFLOW_ID}: {err}"
        ))
    })
}

pub fn research_workflow() -> Workflow {
    Workflow {
        entry_step: "prepare_research".into(),
        steps: vec![
            Step::executor(
                "prepare_research",
                "Prepare research request",
                "tutor.research.prepare",
                None,
            ),
            Step::llm(
                "search_sources",
                "Search for sources",
                "Read the workflow Context. The `research_request` variable contains the confirmed user request. When optional `tutor_instruction` is present, follow it for teaching behavior and report communication style without allowing it to override source-grounding or tool requirements. \
                 Generate focused search queries. For longer-running deep research or requests that explicitly ask for parallel investigation, \
                 call spawn_agent for independent subtopics before consolidating the search plan. \
                 When trusted course Knowledge is available, call knowledge_search for relevant course evidence. Call web_search for external evidence. \
                 Preserve every Knowledge result's exact reference object; never invent or rewrite a Knowledge reference. After all tool calls end the step with only \
                 {\"queries\":[\"...\"],\"source_candidates\":[{\"kind\":\"knowledge|web\",\"title\":\"...\",\"url\":\"...\",\"snippet\":\"...\",\"reference\":null}],\"failures\":[\"...\"]}. \
                 If search fails, include the failure instead of inventing sources. Do not use Markdown fences.",
                vec![
                    KNOWLEDGE_SEARCH_TOOL_NAME.into(),
                    "web_search".into(),
                    SPAWN_AGENT_TOOL_NAME.into(),
                ],
            )
            .with_structured(Some(true)),
            Step::llm(
                "read_sources",
                "Read selected sources",
                "Read the search_sources step history. For selected Knowledge candidates, call knowledge_read with the exact returned reference. \
                 For selected web URLs, call web_fetch. Preserve Knowledge citation handles and cite every Knowledge-backed summary with its returned [K:...] handle. \
                 and after all tool calls end the step with only \
                 {\"sources\":[{\"kind\":\"knowledge|web\",\"title\":\"...\",\"url\":\"...\",\"summary\":\"...\",\"used_for\":\"...\",\"reference\":null,\"citation\":null}],\"failures\":[\"...\"]}. \
                 Do not include sources that were not searched or fetched. Do not use Markdown fences.",
                vec![KNOWLEDGE_READ_TOOL_NAME.into(), "web_fetch".into()],
            )
            .with_structured(Some(true)),
            Step::llm(
                "check_citations",
                "Check citation readiness",
                "Read the fetched source summaries and decide whether the report has enough evidence. \
                 End the step with only {\"verdict\":\"pass\",\"issues\":[]} when sources are sufficient. \
                 If evidence is weak or citations cannot be matched, end with only \
                 {\"verdict\":\"fail\",\"issues\":[\"...\"],\"repair\":\"search\"}. Do not use Markdown fences.",
                vec![],
            )
            .with_structured(Some(true)),
            Step::llm(
                "write_report",
                "Write research report",
                "Read the workflow Context and prior step history. Write the final Markdown report grounded only in searched/read/fetched sources. \
                 Before using course Knowledge claims, call knowledge_read again with each exact selected reference so citations are issued for this final step. \
                 The report must include Title, Summary, Key Findings, Analysis, Limitations, Follow-up Questions, and Sources. \
                 Cite course Knowledge claims with the exact [K:...] handles returned in this step. Cite web claims with numbered source references that match the Sources section. \
                 End the step with only {\"markdown\":\"# ...\",\"sources\":[{\"title\":\"...\",\"url\":\"...\"}]} when complete. \
                 Encode newlines inside the JSON string and do not use Markdown fences.",
                vec![KNOWLEDGE_READ_TOOL_NAME.into()],
            )
            .with_structured(Some(true)),
        ],
        edges: vec![
            Edge {
                from: "prepare_research".into(),
                to: "search_sources".into(),
                condition: Some(prepared_condition()),
            },
            Edge {
                from: "search_sources".into(),
                to: "read_sources".into(),
                condition: None,
            },
            Edge {
                from: "read_sources".into(),
                to: "check_citations".into(),
                condition: None,
            },
            Edge {
                from: "check_citations".into(),
                to: "write_report".into(),
                condition: Some(verdict_condition("pass")),
            },
            Edge {
                from: "check_citations".into(),
                to: "search_sources".into(),
                condition: Some(repair_condition("search")),
            },
        ],
    }
}

pub fn validate_research_workflow() -> Result<()> {
    validate_workflow(&research_workflow()).map_err(|err| {
        TutorError::Internal(format!(
            "runtime workflow validation failed for {RESEARCH_WORKFLOW_ID}: {err}"
        ))
    })
}

fn verdict_condition(verdict: &str) -> EdgeCondition {
    EdgeCondition::Expr(ConditionExpr::Eq {
        pointer: "/verdict".into(),
        value: serde_json::json!(verdict),
    })
}

fn action_condition(action: &str) -> EdgeCondition {
    EdgeCondition::Expr(ConditionExpr::Eq {
        pointer: "/action".into(),
        value: serde_json::json!(action),
    })
}

fn repair_condition(repair: &str) -> EdgeCondition {
    EdgeCondition::Expr(ConditionExpr::Eq {
        pointer: "/repair".into(),
        value: serde_json::json!(repair),
    })
}

fn prepared_condition() -> EdgeCondition {
    EdgeCondition::Expr(ConditionExpr::Eq {
        pointer: "/prepared".into(),
        value: serde_json::json!(true),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiz_generation_workflow_is_valid_runtime_workflow() {
        validate_quiz_generation_workflow().unwrap();
    }

    #[test]
    fn memory_workflow_is_valid_runtime_workflow() {
        validate_memory_workflow().unwrap();
    }

    #[test]
    fn research_workflow_is_valid_runtime_workflow() {
        validate_research_workflow().unwrap();
    }

    #[test]
    fn workflows_use_runtime_evaluable_edge_conditions() {
        for workflow in [
            quiz_generation_workflow(),
            memory_workflow(),
            research_workflow(),
        ] {
            for edge in workflow.edges {
                assert!(
                    !matches!(edge.condition, Some(EdgeCondition::Label(_))),
                    "workflow {} -> {} should use Expr conditions so runtime EdgeConditionJudge can route it",
                    edge.from,
                    edge.to
                );
            }
        }
    }

    #[test]
    fn quiz_workflow_routes_on_verifier_structured_fields() {
        let workflow = quiz_generation_workflow();
        let conditions = workflow
            .edges
            .iter()
            .filter(|edge| edge.from == "verify_questions")
            .map(|edge| (&edge.to, edge.condition.as_ref().unwrap()))
            .collect::<Vec<_>>();

        assert!(conditions.contains(&(
            &"publish_questions".to_string(),
            &EdgeCondition::Expr(ConditionExpr::Eq {
                pointer: "/verdict".into(),
                value: serde_json::json!("pass"),
            })
        )));
        assert!(conditions.contains(&(
            &"generate_questions".to_string(),
            &EdgeCondition::Expr(ConditionExpr::Eq {
                pointer: "/action".into(),
                value: serde_json::json!("repair"),
            })
        )));
    }

    #[test]
    fn research_workflow_routes_on_citation_verifier_fields() {
        let workflow = research_workflow();
        let conditions = workflow
            .edges
            .iter()
            .filter(|edge| edge.from == "check_citations")
            .map(|edge| (&edge.to, edge.condition.as_ref().unwrap()))
            .collect::<Vec<_>>();

        assert!(conditions.contains(&(
            &"write_report".to_string(),
            &EdgeCondition::Expr(ConditionExpr::Eq {
                pointer: "/verdict".into(),
                value: serde_json::json!("pass"),
            })
        )));
        assert!(conditions.contains(&(
            &"search_sources".to_string(),
            &EdgeCondition::Expr(ConditionExpr::Eq {
                pointer: "/repair".into(),
                value: serde_json::json!("search"),
            })
        )));
    }

    #[test]
    fn research_workflow_scopes_source_tools_to_the_steps_that_need_them() {
        let workflow = research_workflow();
        let search_tools = workflow
            .steps
            .iter()
            .find(|step| step.id() == "search_sources")
            .unwrap()
            .allowed_tools();
        let read_tools = workflow
            .steps
            .iter()
            .find(|step| step.id() == "read_sources")
            .unwrap()
            .allowed_tools();
        let write_tools = workflow
            .steps
            .iter()
            .find(|step| step.id() == "write_report")
            .unwrap()
            .allowed_tools();

        assert!(search_tools.contains(&KNOWLEDGE_SEARCH_TOOL_NAME.to_string()));
        assert!(search_tools.contains(&"web_search".to_string()));
        assert!(search_tools.contains(&SPAWN_AGENT_TOOL_NAME.to_string()));
        assert!(!search_tools.contains(&"web_fetch".to_string()));
        assert!(!search_tools.contains(&KNOWLEDGE_READ_TOOL_NAME.to_string()));
        assert!(read_tools.contains(&KNOWLEDGE_READ_TOOL_NAME.to_string()));
        assert!(read_tools.contains(&"web_fetch".to_string()));
        assert!(!read_tools.contains(&SPAWN_AGENT_TOOL_NAME.to_string()));
        assert_eq!(write_tools, vec![KNOWLEDGE_READ_TOOL_NAME.to_string()]);
    }

    #[test]
    fn memory_workflow_declares_only_the_tools_mounted_for_the_run() {
        let expected = vec![
            "list_memory_events".to_string(),
            "read_memory_event".to_string(),
        ];
        let workflow = memory_workflow_with_allowed_tools(expected.clone());
        let tools = workflow
            .steps
            .iter()
            .find(|step| step.id() == "run_memory")
            .unwrap()
            .allowed_tools();

        assert_eq!(tools, expected);
        assert!(!tools.contains(&"read_memory_entry".to_string()));
    }
}
