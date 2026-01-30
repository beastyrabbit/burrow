use crate::config;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct EmbeddingRequest {
    model: String,
    input: String,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    embeddings: Vec<Vec<f32>>,
}

pub async fn generate_embedding(text: &str) -> Result<Vec<f32>, String> {
    let cfg = config::get_config();
    let url = format!("{}/api/embed", cfg.ollama.url);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(cfg.ollama.timeout_secs))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

    let req = EmbeddingRequest {
        model: cfg.ollama.embedding_model.clone(),
        input: text.to_string(),
    };

    let resp = client
        .post(&url)
        .json(&req)
        .send()
        .await
        .map_err(|e| format!("Ollama request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Ollama returned {status}: {body}"));
    }

    let data: EmbeddingResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse Ollama response: {e}"))?;

    data.embeddings
        .into_iter()
        .next()
        .ok_or_else(|| "Ollama returned no embeddings".to_string())
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

pub fn serialize_embedding(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
}

pub fn deserialize_embedding(bytes: &[u8]) -> Vec<f32> {
    if bytes.len() % 4 != 0 {
        eprintln!("Invalid embedding blob length: {}", bytes.len());
        return Vec::new();
    }
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_identical_vectors() {
        let v = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_orthogonal_vectors() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6);
    }

    #[test]
    fn cosine_opposite_vectors() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn cosine_different_lengths_returns_zero() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0, 2.0, 3.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn cosine_empty_returns_zero() {
        let a: Vec<f32> = vec![];
        assert_eq!(cosine_similarity(&a, &a), 0.0);
    }

    #[test]
    fn cosine_zero_vector_returns_zero() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 2.0, 3.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn cosine_known_value() {
        // [1,2,3] · [4,5,6] = 32, |a|=√14, |b|=√77
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![4.0, 5.0, 6.0];
        let expected = 32.0 / (14.0f32.sqrt() * 77.0f32.sqrt());
        let sim = cosine_similarity(&a, &b);
        assert!((sim - expected).abs() < 1e-5);
    }

    #[test]
    fn cosine_negative_values() {
        let a = vec![-1.0, -2.0, -3.0];
        let b = vec![-1.0, -2.0, -3.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn serialize_roundtrip() {
        let original = vec![1.0f32, -2.5, 3.14, 0.0, f32::MAX, f32::MIN];
        let bytes = serialize_embedding(&original);
        let recovered = deserialize_embedding(&bytes);
        assert_eq!(original, recovered);
    }

    #[test]
    fn serialize_empty() {
        let original: Vec<f32> = vec![];
        let bytes = serialize_embedding(&original);
        let recovered = deserialize_embedding(&bytes);
        assert!(recovered.is_empty());
    }

    #[test]
    fn serialize_single() {
        let original = vec![42.0f32];
        let bytes = serialize_embedding(&original);
        assert_eq!(bytes.len(), 4);
        let recovered = deserialize_embedding(&bytes);
        assert_eq!(original, recovered);
    }

    #[test]
    fn serialize_384_dim() {
        let original: Vec<f32> = (0..384).map(|i| i as f32 * 0.01).collect();
        let bytes = serialize_embedding(&original);
        assert_eq!(bytes.len(), 384 * 4);
        let recovered = deserialize_embedding(&bytes);
        assert_eq!(original.len(), recovered.len());
        assert_eq!(original, recovered);
    }
}
