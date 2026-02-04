use aisdk::{
    core::{DynamicModel, LanguageModelRequest},
    providers::OpenRouter,
};

use crate::config::{self, ModelSpec};

#[derive(Debug, Clone)]
pub struct ContextSnippet {
    pub path: String,
    pub preview: String,
}

/// Generate chat response using configured model and provider
pub async fn generate_chat(
    query: &str,
    context_snippets: &[ContextSnippet],
    model_spec: &ModelSpec,
) -> Result<String, String> {
    if crate::actions::dry_run::is_enabled() {
        tracing::debug!(
            query = %crate::actions::dry_run::truncate(query, 80),
            model = %model_spec.name,
            provider = %model_spec.provider,
            "[dry-run] generate_chat"
        );
        return Ok("[dry-run] Chat disabled during testing".into());
    }

    match model_spec.provider.as_str() {
        "ollama" => generate_answer_ollama(query, context_snippets, &model_spec.name).await,
        "openrouter" => generate_answer_openrouter(query, context_snippets, &model_spec.name).await,
        other => Err(format!("Unknown provider: {other}")),
    }
}

/// Chat with large model (uses config routing)
pub async fn chat_large(query: &str, context: &[ContextSnippet]) -> Result<String, String> {
    let cfg = config::get_config();
    generate_chat(query, context, &cfg.models.chat_large).await
}

/// Chat with small model (uses config routing)
#[allow(dead_code)] // Reserved for future use
pub async fn chat_small(query: &str, context: &[ContextSnippet]) -> Result<String, String> {
    let cfg = config::get_config();
    generate_chat(query, context, &cfg.models.chat).await
}

/// Legacy function for backwards compatibility - uses large model
pub async fn generate_answer(
    query: &str,
    context_snippets: &[ContextSnippet],
) -> Result<String, String> {
    chat_large(query, context_snippets).await
}

/// Generate answer using Ollama API
async fn generate_answer_ollama(
    query: &str,
    context_snippets: &[ContextSnippet],
    model: &str,
) -> Result<String, String> {
    let cfg = config::get_config();
    let system_prompt = build_system_prompt(context_snippets);

    let client = reqwest::Client::new();
    let url = format!("{}/api/chat", cfg.ollama.url);

    let messages = vec![
        serde_json::json!({
            "role": "system",
            "content": system_prompt
        }),
        serde_json::json!({
            "role": "user",
            "content": query
        }),
    ];

    let body = serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": false
    });

    let response = client
        .post(&url)
        .timeout(std::time::Duration::from_secs(cfg.ollama.chat_timeout_secs))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Ollama request failed: {e}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Ollama error ({status}): {body}"));
    }

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse Ollama response: {e}"))?;

    json["message"]["content"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "No content in Ollama response".into())
}

/// Generate answer using OpenRouter API
async fn generate_answer_openrouter(
    query: &str,
    context_snippets: &[ContextSnippet],
    model: &str,
) -> Result<String, String> {
    let cfg = config::get_config();

    if cfg.openrouter.api_key.is_empty() {
        return Err(
            "OpenRouter API key not configured. Set BURROW_OPENROUTER_API_KEY or add api_key under [openrouter] in config.toml".into()
        );
    }

    let openrouter_model = OpenRouter::<DynamicModel>::builder()
        .api_key(&cfg.openrouter.api_key)
        .model_name(model)
        .build()
        .map_err(|e| format!("Failed to create OpenRouter model: {e}"))?;

    let system_prompt = build_system_prompt(context_snippets);

    let mut request = LanguageModelRequest::builder()
        .model(openrouter_model)
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

    #[test]
    fn model_spec_helpers() {
        let ollama = ModelSpec::ollama("llama3:8b");
        assert_eq!(ollama.name, "llama3:8b");
        assert_eq!(ollama.provider, "ollama");

        let openrouter = ModelSpec::openrouter("anthropic/claude-sonnet-4");
        assert_eq!(openrouter.name, "anthropic/claude-sonnet-4");
        assert_eq!(openrouter.provider, "openrouter");
    }
}
