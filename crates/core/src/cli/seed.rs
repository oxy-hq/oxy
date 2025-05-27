use crate::{db::client::establish_connection, errors::OxyError, theme::StyledText};
use entity::prelude::{Threads, Users};
use entity::users;
use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter, Set};
use uuid::Uuid;

pub struct TestUser {
    pub email: String,
    pub name: String,
    pub picture: Option<String>,
}

impl TestUser {
    pub fn new(email: &str, name: &str) -> Self {
        Self {
            email: email.to_string(),
            name: name.to_string(),
            picture: Some(format!(
                "https://lh3.googleusercontent.com/a/{}",
                Self::generate_picture_id(email)
            )),
        }
    }

    fn generate_picture_id(email: &str) -> String {
        // Generate a consistent but fake Google profile picture ID
        format!("profile_{}", email.chars().take(8).collect::<String>())
    }
}

pub fn get_test_users() -> Vec<TestUser> {
    vec![
        TestUser::new("alice.smith@company.com", "Alice Smith"),
        TestUser::new("bob.johnson@company.com", "Bob Johnson"),
        TestUser::new("carol.williams@company.com", "Carol Williams"),
        TestUser::new("david.brown@company.com", "David Brown"),
        TestUser::new("eva.davis@company.com", "Eva Davis"),
        TestUser::new("frank.miller@company.com", "Frank Miller"),
        TestUser::new("grace.wilson@company.com", "Grace Wilson"),
        TestUser::new("henry.moore@company.com", "Henry Moore"),
        TestUser::new("iris.taylor@company.com", "Iris Taylor"),
        TestUser::new("jack.anderson@company.com", "Jack Anderson"),
    ]
}

pub async fn seed_test_users() -> Result<Vec<users::Model>, OxyError> {
    let connection = establish_connection().await;
    let test_users = get_test_users();
    let mut created_users = Vec::new();

    println!("🌱 Seeding database with test users...");

    for test_user in test_users {
        // Check if user already exists
        let existing_user = Users::find()
            .filter(users::Column::Email.eq(&test_user.email))
            .one(&connection)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to query existing user: {}", e)))?;

        if existing_user.is_some() {
            println!(
                "  {} User {} already exists, skipping",
                "⏭️".warning(),
                test_user.email
            );
            continue;
        }

        // Create new user
        let new_user = users::ActiveModel {
            id: Set(Uuid::new_v4()),
            email: Set(test_user.email.clone()),
            name: Set(test_user.name),
            picture: Set(test_user.picture),
            created_at: ActiveValue::not_set(), // Will use database default
            last_login_at: ActiveValue::not_set(), // Will use database default
        };

        let user = new_user
            .insert(&connection)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to create user: {}", e)))?;

        println!(
            "  {} Created user: {} ({})",
            "✅".success(),
            user.email,
            user.id
        );
        created_users.push(user);
    }

    println!(
        "🎉 Seeding completed! Created {} new users",
        created_users.len()
    );
    Ok(created_users)
}

pub async fn clear_test_data() -> Result<(), OxyError> {
    let connection = establish_connection().await;

    println!("🧹 Clearing test data...");

    // Delete all threads (due to foreign key cascade, this is safe)
    let threads_deleted = Threads::delete_many()
        .exec(&connection)
        .await
        .map_err(|e| OxyError::DBError(format!("Failed to delete threads: {}", e)))?;
    println!(
        "  {} Deleted {} threads",
        "🗑️".warning(),
        threads_deleted.rows_affected
    );

    // Delete all test users (users with @company.com domain)
    let users_deleted = Users::delete_many()
        .filter(users::Column::Email.like("%@company.com"))
        .exec(&connection)
        .await
        .map_err(|e| OxyError::DBError(format!("Failed to delete test users: {}", e)))?;
    println!(
        "  {} Deleted {} test users",
        "🗑️".warning(),
        users_deleted.rows_affected
    );

    println!("✨ Test data cleared successfully!");
    Ok(())
}

pub async fn create_sample_threads_for_users() -> Result<(), OxyError> {
    use entity::threads;

    let connection = establish_connection().await;

    // Get all test users
    let test_users = Users::find()
        .filter(users::Column::Email.like("%@company.com"))
        .all(&connection)
        .await
        .map_err(|e| OxyError::DBError(format!("Failed to query test users: {}", e)))?;

    if test_users.is_empty() {
        println!("No test users found. Run seed command first.");
        return Ok(());
    }

    println!("📝 Creating sample threads for test users...");

    let sample_threads = [
        (
            "Sales Analysis Q1",
            "SELECT * FROM sales WHERE quarter = 'Q1'",
            "Show me the Q1 sales data",
        ),
        (
            "Customer Demographics",
            "SELECT age_group, count(*) FROM customers GROUP BY age_group",
            "What's the age distribution of our customers?",
        ),
        (
            "Monthly Revenue",
            "SELECT month, SUM(revenue) FROM orders GROUP BY month",
            "Show monthly revenue trends",
        ),
        (
            "Product Performance",
            "SELECT product_name, sales_count FROM products ORDER BY sales_count DESC",
            "Which products are performing best?",
        ),
        (
            "User Engagement",
            "SELECT user_type, AVG(session_duration) FROM user_sessions GROUP BY user_type",
            "How engaged are different user types?",
        ),
    ];

    for (i, user) in test_users.iter().enumerate() {
        // Create 2-3 threads per user
        let num_threads = 2 + (i % 2); // Alternates between 2 and 3 threads

        for j in 0..num_threads {
            let thread_data = &sample_threads[j % sample_threads.len()];

            let new_thread = threads::ActiveModel {
                id: ActiveValue::Set(Uuid::new_v4()),
                user_id: ActiveValue::Set(Some(user.id)),
                title: ActiveValue::Set(format!("{} - {}", thread_data.0, user.name)),
                input: ActiveValue::Set(thread_data.2.to_string()),
                output: ActiveValue::Set(format!(
                    "```sql\n{}\n```\n\nThis query would return the requested data analysis.",
                    thread_data.1
                )),
                source_type: ActiveValue::Set("sql".to_string()),
                source: ActiveValue::Set("sample_analysis.sql".to_string()),
                references: ActiveValue::Set("[]".to_string()),
                created_at: ActiveValue::not_set(),
            };

            let thread = new_thread
                .insert(&connection)
                .await
                .map_err(|e| OxyError::DBError(format!("Failed to create thread: {}", e)))?;

            println!(
                "  {} Created thread '{}' for {}",
                "📄".info(),
                thread.title,
                user.email
            );
        }
    }

    println!("✨ Sample threads created successfully!");
    Ok(())
}
