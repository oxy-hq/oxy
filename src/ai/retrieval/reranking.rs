use arrow::{
    array::{Array, ArrayRef, BooleanArray, Float32Array, RecordBatch, UInt64Array},
    compute::{concat_batches, filter_record_batch, sort_to_indices, take, SortOptions},
    datatypes::{DataType, Field, Schema},
};
use futures::StreamExt;
use lancedb::arrow::RecordBatchStream;
use std::{
    collections::{HashMap, HashSet},
    f32,
    pin::Pin,
    sync::Arc,
};

#[derive(Debug)]
pub struct ReciprocalRankingFusion {
    k: usize,
}

impl Default for ReciprocalRankingFusion {
    fn default() -> Self {
        ReciprocalRankingFusion { k: 60 }
    }
}

impl ReciprocalRankingFusion {
    pub async fn rerank(
        &self,
        vector_results: &mut Pin<Box<dyn RecordBatchStream + Send>>,
        fts_results: &mut Pin<Box<dyn RecordBatchStream + Send>>,
        limit: Option<usize>,
    ) -> anyhow::Result<RecordBatch> {
        let vector_batch = self.to_record_batch(vector_results).await?;
        let fts_batch = self.to_record_batch(fts_results).await?;
        log::info!(
            "Reranking {} vector results and {} fts results",
            vector_batch.num_rows(),
            fts_batch.num_rows()
        );
        let mut rrf_scores = HashMap::new();
        self.compute_relevant_scores(&mut rrf_scores, &vector_batch);
        self.compute_relevant_scores(&mut rrf_scores, &fts_batch);

        let schema = vector_batch.schema();
        let record_batch = concat_batches(&schema, [vector_batch, fts_batch].iter())?;
        let record_batch = self.dedup(&record_batch)?;
        self.sort_by_relevance(&rrf_scores, &record_batch, limit)
    }

    fn compute_relevant_scores(&self, rrf_scores: &mut HashMap<u64, f32>, batch: &RecordBatch) {
        batch
            .column_by_name("_rowid")
            .unwrap()
            .as_any()
            .downcast_ref::<UInt64Array>()
            .unwrap()
            .iter()
            .enumerate()
            .for_each(|(idx, row_id)| {
                if let Some(row_id) = row_id {
                    let row_score = 1_f32 / (idx as f32 + self.k as f32);
                    match rrf_scores.get_mut(&row_id) {
                        Some(score) => *score += row_score,
                        None => {
                            rrf_scores.insert(row_id, row_score);
                        }
                    }
                }
            });
    }

    fn sort_by_relevance(
        &self,
        rrf_scores: &HashMap<u64, f32>,
        record_batch: &RecordBatch,
        limit: Option<usize>,
    ) -> anyhow::Result<RecordBatch> {
        let relevant_scores = record_batch
            .column_by_name("_rowid")
            .unwrap()
            .as_any()
            .downcast_ref::<UInt64Array>()
            .unwrap()
            .iter()
            .map(|x| match x {
                Some(x) => rrf_scores.get(&x).unwrap_or(&0_f32).to_owned(),
                None => 0_f32,
            })
            .collect::<Float32Array>();
        let options = SortOptions {
            descending: true,
            nulls_first: false,
        };
        let indices = sort_to_indices(&relevant_scores, Some(options), limit)?;
        let mut columns = record_batch.columns().to_vec();
        columns.push(Arc::new(relevant_scores));
        let schema = Schema::try_merge(vec![
            Schema::new(record_batch.schema().fields().to_vec()),
            Schema::new(vec![Field::new("relevant", DataType::Float32, false)]),
        ])?;
        RecordBatch::try_new(
            Arc::new(schema),
            columns
                .iter()
                .map(|col| take(col.as_ref(), &indices, None).unwrap())
                .collect::<Vec<ArrayRef>>(),
        )
        .map_err(|err| anyhow::anyhow!("Failed to create record batch: {:?}", err))
    }

    async fn to_record_batch(
        &self,
        vector_results: &mut Pin<Box<dyn RecordBatchStream + Send>>,
    ) -> anyhow::Result<RecordBatch> {
        let schema = vector_results.schema();
        let mut batches = vec![];

        while let Some(vector_batch) = vector_results.next().await {
            match vector_batch {
                Ok(vector_batch) => {
                    batches.push(vector_batch);
                }
                Err(err) => {
                    log::warn!("Failed to get vector batch: {:?}", err);
                }
            }
        }
        let merged_batch = concat_batches(&schema, &batches)?;

        Ok(merged_batch)
    }

    fn dedup(&self, batch: &RecordBatch) -> anyhow::Result<RecordBatch> {
        let mut existing = HashSet::new();
        let mask = batch
            .column_by_name("_rowid")
            .unwrap()
            .as_any()
            .downcast_ref::<UInt64Array>()
            .unwrap()
            .iter()
            .map(|x| match x {
                Some(x) => {
                    if existing.contains(&x) {
                        Some(false)
                    } else {
                        existing.insert(x);
                        return Some(true);
                    }
                }
                None => Some(false),
            })
            .collect::<BooleanArray>();
        let record_batch = filter_record_batch(batch, &mask)?;
        Ok(record_batch)
    }
}
