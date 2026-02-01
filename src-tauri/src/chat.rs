use aisdk::{
    core::{DynamicModel, LanguageModelRequest},
    providers::OpenRouter,
};

use crate::config;

#[derive(Debug, Clone)]
pub struct ContextSnippet {
    pub path: String,
    pub preview: String,
}

pub async fn generate_answer(
    query: &str,
    context_snippets: &[ContextSnippet],
) -> Result<String, String> {
    if crate::actions::dry_run::is_enabled() {
        eprintln!(
            "[dry-run] generate_answer: {}",
            crate::actions::dry_run::truncate(query, 80)
        );
        return Ok("[dry-run] Chat disabled during testing".into());
    }

    let cfg = config::get_config();

    if cfg.openrouter.api_key.is_empty() {
        return Err(
            "OpenRouter API key not configured. Set BURROW_OPENROUTER_API_KEY or add api_key under [openrouter] in config.toml".into()
        );
    }

    let model = OpenRouter::<DynamicModel>::builder()
        .api_key(&cfg.openrouter.api_key)
        .model_name(&cfg.openrouter.model)
        .build()
        .map_err(|e| format!("Failed to create OpenRouter model: {e}"))?;

    let system_prompt = build_system_prompt(context_snippets);

    let mut request = LanguageModelRequest::builder()
        .model(model)
        .system(&system_prompt)
        .prompt(query)
        .build();

    let response = request
        .generate_text()
        .await
        .map_err(|e| format!("Chat generation failed: {e}"))?;

    response
        .text()
        .ok_or_else(|| "No text in chat response".into())
}

fn build_system_prompt(context_snippets: &[ContextSnippet]) -> String {
    if context_snippets.is_empty() {
        return "You are a helpful assistant integrated into Burrow, a desktop application launcher. Answer the user's question concisely.".into();
    }

    let mut prompt = String::from(
        "You are a helpful assistant integrated into Burrow, a desktop application launcher. \
         Answer the user's question using the following file context. Be concise.\n\n\
         --- Context ---\n",
    );

    for snippet in context_snippets {
        prompt.push_str(&format!("\n[{}]\n{}\n", snippet.path, snippet.preview));
    }

    prompt.push_str("\n--- End Context ---\n");
    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_system_prompt_empty_context() {
        let prompt = build_system_prompt(&[]);
        assert!(prompt.contains("helpful assistant"));
        assert!(!prompt.contains("Context"));
    }

    #[test]
    fn build_system_prompt_with_context() {
        let snippets = vec![
            ContextSnippet {
                path: "/home/user/doc.md".into(),
                preview: "Rust is great".into(),
            },
            ContextSnippet {
                path: "/home/user/notes.txt".into(),
                preview: "Setup instructions".into(),
            },
        ];
        let prompt = build_system_prompt(&snippets);
        assert!(prompt.contains("[/home/user/doc.md]"));
        assert!(prompt.contains("Rust is great"));
        assert!(prompt.contains("[/home/user/notes.txt]"));
        assert!(prompt.contains("End Context"));
    }

    #[test]
    fn build_system_prompt_preserves_all_snippets() {
        let snippets: Vec<ContextSnippet> = (0..5)
            .map(|i| ContextSnippet {
                path: format!("/path/{i}.txt"),
                preview: format!("content {i}"),
            })
            .collect();
        let prompt = build_system_prompt(&snippets);
        for i in 0..5 {
            assert!(prompt.contains(&format!("/path/{i}.txt")));
            assert!(prompt.contains(&format!("content {i}")));
        }
    }
}
