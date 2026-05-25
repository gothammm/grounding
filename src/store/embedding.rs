use std::collections::HashMap;

use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

pub struct EmbeddingStore {
    model: Option<TextEmbedding>,
    vectors: Vec<(String, Vec<f32>)>,
    doc_index: HashMap<String, usize>,
}

impl EmbeddingStore {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let model = match TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::BGESmallENV15)
                .with_show_download_progress(false)
        ) {
            Ok(m) => {
                tracing::info!("initialized embedding model (384-dim)");
                Some(m)
            }
            Err(e) => {
                tracing::warn!("failed to initialize embedding model: {}; vector search unavailable", e);
                None
            }
        };
        Self {
            model,
            vectors: Vec::new(),
            doc_index: HashMap::new(),
        }
    }

    pub fn is_available(&self) -> bool {
        self.model.is_some()
    }

    pub fn len(&self) -> usize {
        self.vectors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }

    pub fn add(&mut self, doc_id: &str, embedding: Vec<f32>) {
        if let Some(&pos) = self.doc_index.get(doc_id) {
            self.vectors[pos] = (doc_id.to_string(), embedding);
        } else {
            let pos = self.vectors.len();
            self.doc_index.insert(doc_id.to_string(), pos);
            self.vectors.push((doc_id.to_string(), embedding));
        }
    }

    pub fn remove(&mut self, doc_id: &str) {
        if let Some(&pos) = self.doc_index.get(doc_id) {
            let last = self.vectors.len() - 1;
            if pos != last {
                self.vectors.swap(pos, last);
                let moved_id = self.vectors[pos].0.clone();
                self.doc_index.insert(moved_id, pos);
            }
            self.vectors.pop();
            self.doc_index.remove(doc_id);
        }
    }

    pub fn embed_batch(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        match &self.model {
            Some(model) => {
                let embeddings = model.embed(texts.to_vec(), None)?;
                Ok(embeddings)
            }
            None => Err(anyhow::anyhow!("embedding model not available")),
        }
    }

    pub fn search(&self, query_vec: &[f32], top_k: usize) -> Vec<(String, f32)> {
        if self.vectors.is_empty() {
            return Vec::new();
        }
        let mut scores: Vec<(usize, f32)> = self
            .vectors
            .iter()
            .enumerate()
            .map(|(i, (_, emb))| (i, cosine_similarity(query_vec, emb)))
            .collect();
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        scores.truncate(top_k);
        scores
            .into_iter()
            .map(|(i, score)| (self.vectors[i].0.clone(), score))
            .collect()
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

pub fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(embedding.len() * 4);
    for f in embedding {
        bytes.extend_from_slice(&f.to_le_bytes());
    }
    bytes
}

pub fn bytes_to_embedding(bytes: &[u8]) -> anyhow::Result<Vec<f32>> {
    if !bytes.len().is_multiple_of(4) {
        anyhow::bail!("invalid embedding byte length: {}", bytes.len());
    }
    let chunks = bytes.chunks_exact(4);
    let embedding: Vec<f32> = chunks
        .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
        .collect();
    Ok(embedding)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_roundtrip_bytes() {
        let original = vec![0.1, 0.2, 0.3, -0.4, 1.5];
        let bytes = embedding_to_bytes(&original);
        let restored = bytes_to_embedding(&bytes).unwrap();
        assert_eq!(original.len(), restored.len());
        for (a, b) in original.iter().zip(restored.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn test_invalid_bytes() {
        let bad = vec![0u8, 1, 2];
        let result = bytes_to_embedding(&bad);
        assert!(result.is_err());
    }

    #[test]
    fn test_add_search_remove() {
        let mut store = EmbeddingStore::new();
        // Embeddings can be any dimension since we handle cosine similarity generically
        let emb1 = vec![1.0, 0.0, 0.0];
        let emb2 = vec![0.0, 1.0, 0.0];

        store.add("doc1", emb1.clone());
        store.add("doc2", emb2.clone());
        assert_eq!(store.len(), 2);

        let results = store.search(&emb1, 5);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "doc1");
        assert!((results[0].1 - 1.0).abs() < 1e-6);

        store.remove("doc1");
        assert_eq!(store.len(), 1);

        let results = store.search(&emb1, 5);
        assert!(results.iter().all(|r| r.0 != "doc1"), "removed doc should not appear");
    }

    #[test]
    fn test_empty_store_search() {
        let store = EmbeddingStore::new();
        let results = store.search(&[1.0, 0.0], 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_upsert_same_id() {
        let mut store = EmbeddingStore::new();
        store.add("doc1", vec![1.0, 0.0]);
        store.add("doc1", vec![0.0, 1.0]); // upsert
        assert_eq!(store.len(), 1);
        let results = store.search(&[0.0, 1.0], 5);
        assert_eq!(results[0].0, "doc1");
    }

    #[test]
    fn test_search_sorted_by_score() {
        let mut store = EmbeddingStore::new();
        let query = vec![1.0, 0.0];
        store.add("far", vec![0.0, 1.0]);
        store.add("close", vec![0.9, 0.1]);
        store.add("exact", vec![1.0, 0.0]);
        let results = store.search(&query, 5);
        assert_eq!(results.len(), 3);
        for i in 1..results.len() {
            assert!(results[i - 1].1 >= results[i].1);
        }
        assert_eq!(results[0].0, "exact");
        assert_eq!(results[1].0, "close");
        assert_eq!(results[2].0, "far");
    }
}
