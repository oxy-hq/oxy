//! Intent Classification CLI Commands
//!
//! This module implements the `oxy intent` command which provides unsupervised
//! intent classification for agent questions using clustering techniques.

use crate::errors::OxyError;
use crate::intent::{IntentClassifier, IntentConfig};
use crate::sentry_config;
use crate::theme::StyledText;
use clap::Parser;

/// Intent classification arguments
#[derive(Parser, Debug)]
pub struct IntentArgs {
    /// Intent classification action to perform
    #[clap(subcommand)]
    pub action: IntentAction,
}

/// Available intent classification actions
#[derive(Parser, Debug)]
pub enum IntentAction {
    /// Run the clustering pipeline to discover intents
    ///
    /// Collect questions from traces, generate embeddings,
    /// cluster with HDBSCAN, and label with LLM.
    Cluster {
        /// Minimum cluster size for HDBSCAN
        #[clap(long, default_value_t = 10)]
        min_cluster_size: usize,
        /// Maximum number of questions to process
        #[clap(long, default_value_t = 1000)]
        limit: usize,
    },
    /// Classify a single question
    ///
    /// Test classification against existing clusters.
    Classify {
        /// Question to classify
        question: String,
        /// Enable incremental learning (trigger clustering for unknown questions)
        #[clap(long, default_value_t = false)]
        learn: bool,
    },
    /// Show intent distribution analytics
    ///
    /// Display statistics about classified intents.
    Analytics {
        /// Number of days to include in analytics
        #[clap(long, default_value_t = 30)]
        days: u32,
    },
    /// Show current clusters
    ///
    /// List all discovered intent clusters with sample questions.
    Clusters,
    /// Show outlier questions
    ///
    /// List questions that don't fit any cluster well.
    Outliers {
        /// Maximum number of outliers to show
        #[clap(long, default_value_t = 50)]
        limit: usize,
    },
    /// Show unknown questions status
    ///
    /// Display the number of questions waiting for incremental clustering.
    Pending,
    /// Run incremental clustering
    ///
    /// Process unknown questions and discover new intents or merge into existing clusters.
    Learn,
    /// Test incremental learning with sample data
    ///
    /// Generate ~100 sample analytics questions and classify them,
    /// then optionally run the learn pipeline.
    Test {
        /// Number of test questions to generate (default: 100)
        #[clap(long, default_value_t = 100)]
        count: usize,
        /// Run the learn pipeline after adding test data
        #[clap(long, default_value_t = false)]
        run_learn: bool,
    },
}

/// Handle the intent command and its subcommands
pub async fn handle_intent_command(intent_args: IntentArgs) -> Result<(), OxyError> {
    sentry_config::add_operation_context("intent", None);

    let mut config = IntentConfig::from_env();

    // Update min_cluster_size from CLI args if in Cluster action
    if let IntentAction::Cluster {
        min_cluster_size, ..
    } = &intent_args.action
    {
        config.min_cluster_size = *min_cluster_size;
    }

    let mut classifier = IntentClassifier::new(config.clone()).await.map_err(|e| {
        OxyError::RuntimeError(format!("Failed to initialize intent classifier: {}", e))
    })?;

    match intent_args.action {
        IntentAction::Cluster { limit, .. } => {
            handle_cluster(&mut classifier, limit).await?;
        }
        IntentAction::Classify { question, learn } => {
            handle_classify(&mut classifier, &question, learn).await?;
        }
        IntentAction::Analytics { days } => {
            handle_analytics(&classifier, days).await?;
        }
        IntentAction::Clusters => {
            handle_clusters(&classifier).await?;
        }
        IntentAction::Outliers { limit } => {
            handle_outliers(&classifier, limit).await?;
        }
        IntentAction::Pending => {
            handle_pending(&classifier, &config).await?;
        }
        IntentAction::Learn => {
            handle_learn(&mut classifier).await?;
        }
        IntentAction::Test { count, run_learn } => {
            handle_test(&mut classifier, count, run_learn).await?;
        }
    }

    Ok(())
}

/// Handle the cluster action - run the clustering pipeline
async fn handle_cluster(classifier: &mut IntentClassifier, limit: usize) -> Result<(), OxyError> {
    println!("{}", "🔍 Running intent clustering pipeline...".text());

    let result = classifier
        .run_pipeline(limit)
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Clustering failed: {}", e)))?;

    println!();
    println!("{}", "✅ Pipeline complete!".text());
    println!("   Questions processed: {}", result.questions_processed);
    println!("   Clusters discovered: {}", result.clusters_created);
    println!("   Outliers: {}", result.outliers_count);

    // Show discovered clusters
    let clusters = classifier
        .get_clusters()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Failed to load clusters: {}", e)))?;

    if !clusters.is_empty() {
        println!();
        println!("{}", "📊 Discovered intent clusters:".text());
        for (i, cluster) in clusters.iter().enumerate() {
            println!(
                "   {}. {} - {}",
                i + 1,
                cluster.intent_name,
                cluster.intent_description
            );
        }
    }

    Ok(())
}

/// Handle the classify action - classify a single question
async fn handle_classify(
    classifier: &mut IntentClassifier,
    question: &str,
    learn: bool,
) -> Result<(), OxyError> {
    println!("{}", format!("🎯 Classifying: \"{}\"", question).text());

    if learn {
        // Use classify_with_learning for incremental updates
        let trace_id = format!("cli-{}", uuid::Uuid::new_v4());
        let (classification, added_to_pool) = classifier
            .classify_with_learning(&trace_id, question, "cli", "cli")
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Classification failed: {}", e)))?;

        println!();
        print_classification_result(&classification);

        if added_to_pool {
            let unknown_count = classifier.get_unknown_count().await.unwrap_or(0);
            println!();
            println!(
                "{}",
                format!(
                    "📥 Added to learning queue (low confidence). Unknown count: {}",
                    unknown_count
                )
                .tertiary()
            );
        }
    } else {
        let classification = classifier
            .classify(question)
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Classification failed: {}", e)))?;

        println!();
        print_classification_result(&classification);
    }

    Ok(())
}

/// Print classification result in a formatted way
fn print_classification_result(classification: &crate::intent::IntentClassification) {
    if classification.intent_name != "unknown" {
        println!(
            "{}",
            format!("Intent: {}", classification.intent_name).text()
        );
        println!("   Description: {}", classification.intent_description);
        println!("   Confidence: {:.2}%", classification.confidence * 100.0);
    } else {
        println!("{}", "⚠️  No matching intent found (outlier)".tertiary());
        println!(
            "   Closest similarity: {:.2}%",
            classification.confidence * 100.0
        );
    }
}

/// Handle the analytics action - show intent distribution
async fn handle_analytics(classifier: &IntentClassifier, days: u32) -> Result<(), OxyError> {
    println!(
        "{}",
        format!("📈 Intent analytics (last {} days)", days).text()
    );

    let analytics = classifier
        .get_analytics(days)
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Failed to get analytics: {}", e)))?;

    println!();

    if analytics.is_empty() {
        println!(
            "{}",
            "No classifications yet. Run 'oxy intent cluster' first.".tertiary()
        );
    } else {
        let total: u64 = analytics.iter().map(|a| a.count).sum();
        println!("Total classified: {}", total);
        println!("Unique intents: {}", analytics.len());
        println!();
        println!("{}", "Intent distribution:".text());
        for item in &analytics {
            println!(
                "   {} - {} ({:.1}%)",
                item.intent_name, item.count, item.percentage
            );
        }
    }

    Ok(())
}

/// Handle the clusters action - list all clusters
async fn handle_clusters(classifier: &IntentClassifier) -> Result<(), OxyError> {
    println!("{}", "📋 Current intent clusters".text());

    let clusters = classifier
        .get_clusters()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Failed to load clusters: {}", e)))?;

    if clusters.is_empty() {
        println!();
        println!(
            "{}",
            "No clusters found. Run 'oxy intent cluster' first.".tertiary()
        );
    } else {
        println!();
        for cluster in clusters {
            println!(
                "{}",
                format!(
                    "🏷️  {} (samples: {})",
                    cluster.intent_name,
                    cluster.sample_questions.len()
                )
                .text()
            );
            println!("   {}", cluster.intent_description.tertiary());
            for sample in &cluster.sample_questions {
                println!("     • {}", sample);
            }
            println!();
        }
    }

    Ok(())
}

/// Handle the outliers action - show outlier questions
async fn handle_outliers(classifier: &IntentClassifier, limit: usize) -> Result<(), OxyError> {
    println!("{}", format!("🔮 Outlier questions (top {})", limit).text());

    let outliers = classifier
        .get_outliers(limit)
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Failed to get outliers: {}", e)))?;

    if outliers.is_empty() {
        println!();
        println!("{}", "No outliers found.".tertiary());
    } else {
        println!();
        for (i, (question, _trace_id)) in outliers.iter().enumerate() {
            println!("   {}. {}", i + 1, question);
        }
        println!();
        println!(
            "{}",
            "These questions don't match existing intents well.".tertiary()
        );
    }

    Ok(())
}

/// Handle the pending action - show unknown questions status
async fn handle_pending(
    classifier: &IntentClassifier,
    config: &IntentConfig,
) -> Result<(), OxyError> {
    println!("{}", "📥 Unknown questions status".text());

    let count = classifier
        .get_unknown_count()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Failed to get unknown count: {}", e)))?;

    println!();
    println!("   Unknown questions: {}", count);

    let threshold = config.learning_pool_threshold;
    if count >= threshold {
        println!(
            "{}",
            format!(
                "   ⚡ Pool has reached threshold ({}). Run 'oxy intent learn' to process.",
                threshold
            )
            .text()
        );
    } else {
        println!(
            "{}",
            format!("   Pool will auto-process at {} items", threshold).tertiary()
        );
    }

    Ok(())
}

/// Handle the learn action - run incremental clustering
async fn handle_learn(classifier: &mut IntentClassifier) -> Result<(), OxyError> {
    println!("{}", "🧠 Running incremental learning...".text());

    let result = classifier
        .run_incremental_clustering()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Incremental learning failed: {}", e)))?;

    println!();
    if result.items_processed == 0 {
        println!("{}", "No unknown questions to process.".tertiary());
    } else {
        println!("{}", "✅ Incremental learning complete!".text());
        println!("   Items processed: {}", result.items_processed);
        println!("   New clusters created: {}", result.new_clusters);
        println!("   Merged into existing: {}", result.merged_count);
        println!("   Outliers: {}", result.outliers_count);

        // Show updated clusters
        let clusters = classifier
            .get_clusters()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to load clusters: {}", e)))?;

        if !clusters.is_empty() {
            println!();
            println!("{}", "📊 Current clusters:".text());
            for (i, cluster) in clusters.iter().enumerate() {
                println!(
                    "   {}. {} ({} samples)",
                    i + 1,
                    cluster.intent_name,
                    cluster.sample_questions.len()
                );
            }
        }
    }

    Ok(())
}

/// Handle the test action - generate test questions for incremental learning
async fn handle_test(
    classifier: &mut IntentClassifier,
    count: usize,
    run_learn: bool,
) -> Result<(), OxyError> {
    println!(
        "{}",
        format!(
            "🧪 Generating {} test questions for incremental learning...",
            count
        )
        .text()
    );

    let test_questions = get_test_questions();

    // Generate the requested number of questions (cycling through if needed)
    let mut questions_to_add = Vec::with_capacity(count);
    for i in 0..count {
        let base_question = test_questions[i % test_questions.len()];
        // Add slight variations for larger counts
        let question = if i >= test_questions.len() {
            format!("{} (variation {})", base_question, i / test_questions.len())
        } else {
            base_question.to_string()
        };
        questions_to_add.push(question);
    }

    // Classify questions (may add to unknown pool)
    let mut added = 0;
    for question in &questions_to_add {
        let trace_id = format!("test-{}", uuid::Uuid::new_v4());
        let (_classification, was_added) = classifier
            .classify_with_learning(&trace_id, question, "cli", "test")
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to add test question: {}", e)))?;
        if was_added {
            added += 1;
        }
    }

    println!();
    println!(
        "{}",
        format!(
            "✅ Processed {} test questions, {} had low confidence",
            count, added
        )
        .text()
    );

    let unknown_count = classifier.get_unknown_count().await.unwrap_or(0);
    println!("   Current unknown questions count: {}", unknown_count);

    if run_learn {
        println!();
        println!("{}", "🧠 Running incremental learning...".text());

        let result = classifier
            .run_incremental_clustering()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Incremental learning failed: {}", e)))?;

        println!();
        if result.items_processed == 0 {
            println!("{}", "No unknown questions to process.".tertiary());
        } else {
            println!("{}", "✅ Incremental learning complete!".text());
            println!("   Items processed: {}", result.items_processed);
            println!("   New clusters created: {}", result.new_clusters);
            println!("   Merged into existing: {}", result.merged_count);
            println!("   Outliers: {}", result.outliers_count);

            let clusters = classifier
                .get_clusters()
                .await
                .map_err(|e| OxyError::RuntimeError(format!("Failed to load clusters: {}", e)))?;

            if !clusters.is_empty() {
                println!();
                println!("{}", "📊 Discovered clusters:".text());
                for (i, cluster) in clusters.iter().enumerate() {
                    println!(
                        "   {}. {} ({} samples)",
                        i + 1,
                        cluster.intent_name,
                        cluster.sample_questions.len()
                    );
                    println!("      {}", cluster.intent_description.tertiary());
                }
            }
        }
    } else {
        println!();
        println!(
            "{}",
            "Run 'oxy intent learn' to process unknown questions.".tertiary()
        );
    }

    Ok(())
}

/// Get sample analytics questions for testing
fn get_test_questions() -> Vec<&'static str> {
    vec![
        // User metrics questions
        "how many users signed up last week?",
        "what is our total user count?",
        "how many active users do we have?",
        "what's the user growth rate?",
        "how many new users joined today?",
        "show me daily active users trend",
        "what's our user retention rate?",
        "how many users churned last month?",
        "what percentage of users are premium?",
        "how many users logged in this week?",
        "what's the average session duration per user?",
        "how many users completed onboarding?",
        "what's our monthly active user count?",
        "show me user engagement metrics",
        "how many users are in each tier?",
        // Revenue questions
        "what is our total revenue?",
        "show me revenue by month",
        "what's the average order value?",
        "how much revenue did we make today?",
        "what's our MRR?",
        "show me revenue growth trend",
        "what's revenue by product category?",
        "how does this quarter compare to last?",
        "what's our ARR?",
        "show me revenue per customer",
        "what's the lifetime value of customers?",
        "how much revenue from subscriptions?",
        "what's our gross margin?",
        "show me revenue by region",
        "what's the revenue forecast?",
        // Sales questions
        "how many sales did we make today?",
        "what's the conversion rate?",
        "show me sales by region",
        "what products are selling best?",
        "what's our average deal size?",
        "how many deals are in the pipeline?",
        "what's the win rate?",
        "show me sales performance by rep",
        "how long is our sales cycle?",
        "what's the close rate this month?",
        "show me top performing products",
        "how many leads converted today?",
        "what's the average time to close?",
        "show me sales trends over time",
        "which channels drive the most sales?",
        // Customer support questions
        "how many support tickets were opened?",
        "what's the average response time?",
        "show me ticket volume by category",
        "what's our CSAT score?",
        "how many tickets are unresolved?",
        "what's the average resolution time?",
        "show me support metrics dashboard",
        "which issues are most common?",
        "how many escalations this week?",
        "what's the first response time?",
        "show me agent performance",
        "how many tickets per customer?",
        "what's the ticket backlog?",
        "show me support trends",
        "which products generate most tickets?",
        // Marketing questions
        "what's our website traffic?",
        "how many leads did we generate?",
        "what's the cost per acquisition?",
        "show me campaign performance",
        "what's our email open rate?",
        "how many conversions from ads?",
        "what's our SEO ranking?",
        "show me marketing ROI",
        "which channels perform best?",
        "what's the click-through rate?",
        "how many social media followers?",
        "what's our brand awareness score?",
        "show me funnel metrics",
        "how effective is content marketing?",
        "what's our cost per lead?",
        // Product analytics
        "which features are most used?",
        "what's the feature adoption rate?",
        "show me product usage metrics",
        "how many API calls today?",
        "what's the error rate?",
        "show me performance metrics",
        "which pages have highest bounce rate?",
        "what's the average load time?",
        "how many bugs were reported?",
        "show me feature engagement",
        "what's the completion rate?",
        "which workflows are most common?",
        "how many integrations are active?",
        "show me mobile vs desktop usage",
        "what's the crash rate?",
        // Financial questions
        "what are our operating expenses?",
        "show me cash flow statement",
        "what's our burn rate?",
        "how much runway do we have?",
        "what's the profit margin?",
        "show me expense breakdown",
        "what's our debt to equity ratio?",
        "how much did we spend on marketing?",
        "what are projected expenses?",
        "show me budget vs actual",
    ]
}
