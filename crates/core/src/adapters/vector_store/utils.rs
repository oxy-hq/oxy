use crate::adapters::vector_store::types::Document;

pub fn build_content_for_llm_retrieval(document: &Document) -> String {
    let inclusions: Vec<String> = document.retrieval_inclusions
        .iter()
        .map(|r| r.embedding_content.clone())
        .collect();
    let exclusions: Vec<String> = document.retrieval_exclusions
        .iter()
        .map(|r| r.embedding_content.clone())
        .collect();
    
    build_inclusion_exclusion_summary(&inclusions, &exclusions)
} 

pub fn build_inclusion_exclusion_summary(
  inclusions: &[String],
  exclusions: &[String],
) -> String {
  let mut content_parts = vec![];
  
  for inclusion in inclusions {
      content_parts.push(inclusion.clone());
  }
  
  // NOTE: exclusions should already be excluded via epsilon ball filtering.
  //       but in the event that the filtering fails work right, this is a
  //       final guard rail that should prevent the LLM from choosing the tool
  //       for an excluded prompt.
  for exclusion in exclusions {
      content_parts.push(format!("DO NOT USE FOR PROMPT: {}", exclusion));
  }
  
  content_parts.join("\n")
}
