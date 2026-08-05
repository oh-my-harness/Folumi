use crate::session::AssistantSessionConfig;

pub const DEFAULT_ASSISTANT_NAME: &str = "Folumi Assistant";
pub const DEFAULT_ASSISTANT_INSTRUCTIONS: &str = "You are Folumi, a local-first personal knowledge assistant. Answer directly and naturally. Be concise by default, and expand when the user asks or the task requires more detail.";

pub(crate) fn normalized_assistant_name(name: &str) -> String {
    let name = name.trim();
    if name.is_empty() {
        DEFAULT_ASSISTANT_NAME.into()
    } else {
        name.into()
    }
}

pub(crate) fn normalized_assistant_instructions(instructions: &str) -> String {
    let instructions = instructions.trim();
    if instructions.is_empty() {
        DEFAULT_ASSISTANT_INSTRUCTIONS.into()
    } else {
        instructions.into()
    }
}

pub(crate) fn normalize_assistant_profile(
    name: &str,
    instructions: &str,
) -> AssistantSessionConfig {
    AssistantSessionConfig {
        name: normalized_assistant_name(name),
        instructions: normalized_assistant_instructions(instructions),
    }
}

pub(crate) fn assistant_profile_instruction(assistant: &AssistantSessionConfig) -> String {
    let assistant = normalize_assistant_profile(&assistant.name, &assistant.instructions);
    format!(
        "## Assistant Profile\n\nAssistant name: {}\n\n{}",
        assistant.name, assistant.instructions
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_profile_uses_visible_product_default() {
        let profile = normalize_assistant_profile(" ", "\n");
        assert_eq!(profile.name, DEFAULT_ASSISTANT_NAME);
        assert_eq!(profile.instructions, DEFAULT_ASSISTANT_INSTRUCTIONS);
        assert!(
            assistant_profile_instruction(&profile)
                .contains("local-first personal knowledge assistant")
        );
    }

    #[test]
    fn custom_profile_is_the_identity_source() {
        let profile = normalize_assistant_profile("Mori", "Use short, practical answers.");
        let instruction = assistant_profile_instruction(&profile);
        assert!(instruction.contains("Assistant name: Mori"));
        assert!(instruction.contains("Use short, practical answers."));
        assert!(!instruction.contains("knowledgeable tutor"));
    }
}
