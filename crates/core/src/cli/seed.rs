use crate::{db::client::establish_connection, errors::OxyError, theme::StyledText};
use entity::prelude::{Threads, Users};
use entity::users::{self, UserRole, UserStatus};
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
        TestUser::new("guest@oxy.local", "Guest User"),
        TestUser::new("alice.smith@company.com", "Alice Smith"),
        TestUser::new("bob.johnson@company.com", "Bob Johnson"),
    ]
}

pub async fn seed_test_users() -> Result<Vec<users::Model>, OxyError> {
    let connection = establish_connection().await?;
    let test_users = get_test_users();
    let mut created_users = Vec::new();

    println!("🌱 Seeding database with test users...");

    for test_user in test_users {
        // Check if user already exists
        let existing_user = Users::find()
            .filter(users::Column::Email.eq(&test_user.email))
            .one(&connection)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to query existing user: {e}")))?;

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
            password_hash: ActiveValue::not_set(),
            email_verified: Set(true),
            email_verification_token: ActiveValue::not_set(),
            created_at: ActiveValue::not_set(), // Will use database default
            last_login_at: ActiveValue::not_set(), // Will use database default
            role: ActiveValue::Set(UserRole::Member),
            status: ActiveValue::Set(UserStatus::Active),
        };

        let user = new_user
            .insert(&connection)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to create user: {e}")))?;

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
    let connection = establish_connection().await?;

    println!("🧹 Clearing test data...");

    // Delete all threads (due to foreign key cascade, this is safe)
    let threads_deleted = Threads::delete_many()
        .exec(&connection)
        .await
        .map_err(|e| OxyError::DBError(format!("Failed to delete threads: {e}")))?;
    println!(
        "  {} Deleted {} threads",
        "🗑️".warning(),
        threads_deleted.rows_affected
    );

    // Delete all test users (users with @company.com domain and guest@oxy.local)
    let users_deleted = Users::delete_many()
        .filter(
            users::Column::Email
                .like("%@company.com")
                .or(users::Column::Email.eq("guest@oxy.local")),
        )
        .exec(&connection)
        .await
        .map_err(|e| OxyError::DBError(format!("Failed to delete test users: {e}")))?;
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

    let connection = establish_connection().await?;

    // Get all test users
    let test_users = Users::find()
        .filter(
            users::Column::Email
                .like("%@company.com")
                .or(users::Column::Email.eq("guest@oxy.local")),
        )
        .all(&connection)
        .await
        .map_err(|e| OxyError::DBError(format!("Failed to query test users: {e}")))?;

    if test_users.is_empty() {
        println!("No test users found. Run seed command first.");
        return Ok(());
    }

    println!("📝 Creating 1000 sample threads for each test user...");

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

    for (user_index, user) in test_users.iter().enumerate() {
        println!(
            "  {} Creating 1000 threads for {} ({}/{})",
            "🔄".info(),
            user.email,
            user_index + 1,
            test_users.len()
        );

        for thread_index in 0..1000 {
            let thread_data = &sample_threads[thread_index % sample_threads.len()];

            let new_thread = threads::ActiveModel {
                id: ActiveValue::Set(Uuid::new_v4()),
                user_id: ActiveValue::Set(Some(user.id)),
                title: ActiveValue::Set(format!(
                    "{} #{} - {}",
                    thread_data.0,
                    thread_index + 1,
                    user.name
                )),
                input: ActiveValue::Set(thread_data.2.to_string()),
                output: ActiveValue::Set(format!(
                    "```sql\n{}\n```\n\nThis query would return the requested data analysis.",
                    thread_data.1
                )),
                source_type: ActiveValue::Set("sql".to_string()),
                source: ActiveValue::Set("sample_analysis.sql".to_string()),
                references: ActiveValue::Set("[]".to_string()),
                created_at: ActiveValue::not_set(),
                is_processing: ActiveValue::Set(false),
                project_id: ActiveValue::Set(
                    "00000000-0000-0000-0000-000000000000"
                        .parse::<Uuid>()
                        .unwrap(),
                ),
                sandbox_info: ActiveValue::Set(None),
            };

            let _thread = new_thread
                .insert(&connection)
                .await
                .map_err(|e| OxyError::DBError(format!("Failed to create thread: {e}")))?;

            // Progress reporting every 100 threads
            if (thread_index + 1) % 100 == 0 {
                println!(
                    "    {} Created {}/1000 threads for {}",
                    "📄".info(),
                    thread_index + 1,
                    user.email
                );
            }
        }

        println!(
            "  {} Completed all 1000 threads for {}",
            "✅".success(),
            user.email
        );
    }

    println!("✨ Sample threads created successfully!");
    Ok(())
}
