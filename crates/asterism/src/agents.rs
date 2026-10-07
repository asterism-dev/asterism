use asterism_plugin::protocol::LaunchMode;

use crate::plugins::manifest::AgentDecl;

pub fn on_path(binary: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(binary).is_file()))
}

/// Expands a static agent's templates; `None` when it cannot resume.
pub fn static_argv(
    agent: &AgentDecl,
    mode: LaunchMode,
    args: &[String],
    prompt: Option<&str>,
    agent_ref: Option<&str>,
) -> Option<Vec<String>> {
    let mut argv = Vec::new();
    let mut expand = |tokens: &[String]| {
        for token in tokens {
            match token.as_str() {
                "{binary}" => argv.push(agent.binary.clone()),
                "{args...}" => argv.extend(args.iter().cloned()),
                "{prompt}" => argv.push(prompt.unwrap_or_default().to_string()),
                "{agent_ref}" => argv.push(agent_ref.unwrap_or_default().to_string()),
                other => argv.push(other.to_string()),
            }
        }
    };
    match mode {
        LaunchMode::Start => {
            expand(&agent.start);
            if prompt.is_some() {
                expand(&agent.prompt);
            }
        }
        LaunchMode::Resume => {
            if agent.resume.is_empty() || agent_ref.is_none() {
                return None;
            }
            expand(&agent.resume);
        }
    }
    Some(argv)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::manifest::LaunchKind;

    fn aider() -> AgentDecl {
        AgentDecl {
            id: "aider".into(),
            display_name: None,
            binary: "aider".into(),
            launch: LaunchKind::Static,
            waiting_patterns: vec![],
            settings: vec![],
            reserved_args: vec![],
            start: vec!["{binary}".into(), "{args...}".into()],
            prompt: vec!["--message".into(), "{prompt}".into()],
            resume: vec!["{binary}".into(), "--restore".into(), "{agent_ref}".into()],
        }
    }

    #[test]
    fn static_templates_expand_args_prompt_and_resume() {
        let args = ["--model".to_string(), "x".into()];
        assert_eq!(
            static_argv(&aider(), LaunchMode::Start, &args, Some("go"), None).unwrap(),
            ["aider", "--model", "x", "--message", "go"]
        );
        assert_eq!(
            static_argv(&aider(), LaunchMode::Start, &[], None, None).unwrap(),
            ["aider"]
        );
        assert_eq!(
            static_argv(&aider(), LaunchMode::Resume, &[], None, Some("r1")).unwrap(),
            ["aider", "--restore", "r1"]
        );
        let no_resume = AgentDecl {
            resume: vec![],
            ..aider()
        };
        assert_eq!(
            static_argv(&no_resume, LaunchMode::Resume, &[], None, Some("r1")),
            None
        );
    }

    #[test]
    fn placeholders_are_whole_tokens_only() {
        let prompt = "use {agent_ref} and {binary}";
        assert_eq!(
            static_argv(&aider(), LaunchMode::Start, &[], Some(prompt), Some("r1")).unwrap(),
            ["aider", "--message", prompt]
        );
    }

    #[test]
    fn on_path_finds_sh() {
        assert!(on_path("sh"));
        assert!(!on_path("definitely-not-a-binary-asterism"));
    }
}
