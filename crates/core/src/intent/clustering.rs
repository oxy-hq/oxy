//! HDBSCAN-inspired clustering for intent classification
//!
//! This module implements a simplified density-based clustering algorithm
//! inspired by HDBSCAN. It automatically determines the number of clusters
//! and marks outliers as noise (cluster_id = -1).
//!
//! The algorithm:
//! 1. Build a distance matrix using cosine distance
//! 2. Find core points (points with enough neighbors within epsilon)
//! 3. Grow clusters from core points
//! 4. Mark remaining points as noise

use super::embedding::cosine_similarity;
use super::types::Cluster;

/// Simple density-based clustering result
pub struct ClusteringResult {
    /// Cluster labels for each point (-1 = noise/outlier)
    pub labels: Vec<i32>,
    /// Number of clusters found (excluding noise)
    pub num_clusters: usize,
}

/// Perform density-based clustering on embeddings
///
/// This is a simplified HDBSCAN-like algorithm that:
/// - Automatically finds clusters based on density
/// - Marks outliers as noise (label = -1)
/// - Requires no predefined number of clusters
///
/// # Arguments
/// * `embeddings` - Vector of embedding vectors
/// * `min_cluster_size` - Minimum points to form a cluster
///
/// # Returns
/// ClusteringResult with labels for each point
pub fn cluster_embeddings(embeddings: &[Vec<f32>], min_cluster_size: usize) -> ClusteringResult {
    let n = embeddings.len();
    if n == 0 {
        return ClusteringResult {
            labels: vec![],
            num_clusters: 0,
        };
    }

    if n < min_cluster_size {
        // Not enough points for any cluster
        return ClusteringResult {
            labels: vec![-1; n],
            num_clusters: 0,
        };
    }

    // Step 1: Build similarity matrix
    let similarities = build_similarity_matrix(embeddings);

    // Step 2: Find neighbors for each point
    // Use adaptive threshold based on the distribution of similarities
    let threshold = compute_adaptive_threshold(&similarities, min_cluster_size);
    let neighbors = find_neighbors(&similarities, threshold);

    // Step 3: Identify core points (points with enough neighbors)
    let core_points: Vec<bool> = neighbors
        .iter()
        .map(|n| n.len() >= min_cluster_size)
        .collect();

    // Step 4: Grow clusters from core points using BFS
    let mut labels = vec![-1i32; n];
    let mut current_cluster = 0i32;

    for i in 0..n {
        if !core_points[i] || labels[i] != -1 {
            continue;
        }

        // Start a new cluster from this core point
        let mut queue = vec![i];
        labels[i] = current_cluster;

        while let Some(point) = queue.pop() {
            for &neighbor in &neighbors[point] {
                if labels[neighbor] == -1 {
                    labels[neighbor] = current_cluster;
                    // Only expand from core points
                    if core_points[neighbor] {
                        queue.push(neighbor);
                    }
                }
            }
        }

        current_cluster += 1;
    }

    // Step 5: Validate cluster sizes and merge small clusters into noise
    let num_clusters = current_cluster as usize;
    let mut cluster_sizes = vec![0usize; num_clusters];
    for &label in &labels {
        if label >= 0 {
            cluster_sizes[label as usize] += 1;
        }
    }

    // Mark points in too-small clusters as noise
    for label in &mut labels {
        if *label >= 0 && cluster_sizes[*label as usize] < min_cluster_size {
            *label = -1;
        }
    }

    // Renumber clusters to be contiguous
    let (final_labels, final_count) = renumber_clusters(&labels);

    ClusteringResult {
        labels: final_labels,
        num_clusters: final_count,
    }
}

/// Build a similarity matrix using cosine similarity
fn build_similarity_matrix(embeddings: &[Vec<f32>]) -> Vec<Vec<f32>> {
    let n = embeddings.len();
    let mut matrix = vec![vec![0.0f32; n]; n];

    for i in 0..n {
        matrix[i][i] = 1.0; // Self-similarity
        for j in (i + 1)..n {
            let sim = cosine_similarity(&embeddings[i], &embeddings[j]);
            matrix[i][j] = sim;
            matrix[j][i] = sim;
        }
    }

    matrix
}

/// Compute an adaptive similarity threshold
fn compute_adaptive_threshold(similarities: &[Vec<f32>], _min_cluster_size: usize) -> f32 {
    // Collect all non-self similarities
    let mut all_sims: Vec<f32> = Vec::new();
    for (i, row) in similarities.iter().enumerate() {
        for (j, &sim) in row.iter().enumerate() {
            if i != j {
                all_sims.push(sim);
            }
        }
    }

    if all_sims.is_empty() {
        return 0.5;
    }

    all_sims.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

    // Use a threshold that captures roughly the top similarities
    // This helps find dense regions
    let target_idx = (all_sims.len() as f32 * 0.1) as usize; // Top 10%
    let base_threshold = all_sims.get(target_idx).copied().unwrap_or(0.5);

    // Ensure threshold is reasonable
    base_threshold.max(0.5).min(0.95)
}

/// Find neighbors for each point above the similarity threshold
fn find_neighbors(similarities: &[Vec<f32>], threshold: f32) -> Vec<Vec<usize>> {
    similarities
        .iter()
        .enumerate()
        .map(|(i, row)| {
            row.iter()
                .enumerate()
                .filter(|&(j, &sim)| i != j && sim >= threshold)
                .map(|(j, _)| j)
                .collect()
        })
        .collect()
}

/// Renumber clusters to be contiguous (0, 1, 2, ...) and count them
fn renumber_clusters(labels: &[i32]) -> (Vec<i32>, usize) {
    use std::collections::HashMap;

    let mut mapping: HashMap<i32, i32> = HashMap::new();
    let mut next_id = 0i32;

    let new_labels: Vec<i32> = labels
        .iter()
        .map(|&label| {
            if label == -1 {
                -1
            } else {
                *mapping.entry(label).or_insert_with(|| {
                    let id = next_id;
                    next_id += 1;
                    id
                })
            }
        })
        .collect();

    (new_labels, next_id as usize)
}

/// Extract clusters from labels and embeddings
pub fn extract_clusters(
    labels: &[i32],
    embeddings: &[Vec<f32>],
    questions: &[String],
) -> Vec<Cluster> {
    use std::collections::HashMap;

    let mut cluster_data: HashMap<i32, (Vec<Vec<f32>>, Vec<String>)> = HashMap::new();

    for (i, &label) in labels.iter().enumerate() {
        if label >= 0 {
            let entry = cluster_data
                .entry(label)
                .or_insert_with(|| (vec![], vec![]));
            entry.0.push(embeddings[i].clone());
            entry.1.push(questions[i].clone());
        }
    }

    let mut clusters: Vec<Cluster> = cluster_data
        .into_iter()
        .map(|(id, (embs, qs))| {
            let centroid = Cluster::calculate_centroid(&embs);
            Cluster {
                id,
                embeddings: embs,
                questions: qs,
                centroid,
            }
        })
        .collect();

    clusters.sort_by_key(|c| c.id);
    clusters
}
