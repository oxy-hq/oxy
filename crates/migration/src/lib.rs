pub use sea_orm_migration::prelude::*;

mod m20220101_000001_create_table;
mod m20241111_110133_add_agent_to_conversation;
mod m20241112_035850_add_message;
mod m20250307_090813_add_threads;
mod m20250318_230139_add_thread_references;
mod m20250501_215840_add_tasks;
mod m20250519_011103_add_workflow_to_threads;
mod m20250522_011451_drop_messages_and_conversations;
mod m20250523_123859_add_users_table;
mod m20250523_123900_add_user_id_to_threads;
mod m20250527_005652_create_table_messages;
mod m20250609_000001_create_api_keys_table;
#[allow(non_snake_case)]
mod m20250609_015141_Add_artifacts;
mod m20250611_015638_add_tokens_to_messages;
mod m20250613_090405_add_auth_fields_to_users;
mod m20250618_102934_create_github_config_table;
mod m20250624_100000_add_role_to_users;
mod m20250625_000001_add_status_to_users;
mod m20250625_151048_add_is_processing_to_thread;
mod m20250626_000001_create_secrets_table;
mod m20250708_021201_create_logs_table;
mod m20250727_150336_add_run_model;
mod m20250811_084647_add_organizations_table;
mod m20250811_084822_add_organization_users_table;
mod m20250811_085101_projects_table;
mod m20250811_090444_branches_table;
mod m20250813_071440_add_project_id_to_threads_table;
mod m20250813_071500_add_project_id_to_secrets_table;
mod m20250813_071600_add_project_id_to_runs_table;
mod m20250819_020551_add_project_id_to_api_keys_table;
mod m20250819_084109_fix_root_replay_ref_type;
mod m20250902_073902_add_active_branch_id_to_projects_table;
mod m20250902_080016_add_branch_id_to_runs_table;
mod m20250902_080217_add_project_branch_id_to_run_index_runs_table;
mod m20250923_064746_change_organization_to_workspace;
mod m20250924_015621_create_project_repos_and_update_projects;
mod m20250929_081920_create_git_namespaces_table;
mod m20251009_020233_add_run_variables_and_output;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20220101_000001_create_table::Migration),
            Box::new(m20241111_110133_add_agent_to_conversation::Migration),
            Box::new(m20241112_035850_add_message::Migration),
            Box::new(m20250307_090813_add_threads::Migration),
            Box::new(m20250318_230139_add_thread_references::Migration),
            Box::new(m20250501_215840_add_tasks::Migration),
            Box::new(m20250519_011103_add_workflow_to_threads::Migration),
            Box::new(m20250522_011451_drop_messages_and_conversations::Migration),
            Box::new(m20250523_123859_add_users_table::Migration),
            Box::new(m20250523_123900_add_user_id_to_threads::Migration),
            Box::new(m20250527_005652_create_table_messages::Migration),
            Box::new(m20250609_000001_create_api_keys_table::Migration),
            Box::new(m20250609_015141_Add_artifacts::Migration),
            Box::new(m20250611_015638_add_tokens_to_messages::Migration),
            Box::new(m20250613_090405_add_auth_fields_to_users::Migration),
            Box::new(m20250618_102934_create_github_config_table::Migration),
            Box::new(m20250624_100000_add_role_to_users::Migration),
            Box::new(m20250625_000001_add_status_to_users::Migration),
            Box::new(m20250625_151048_add_is_processing_to_thread::Migration),
            Box::new(m20250626_000001_create_secrets_table::Migration),
            Box::new(m20250708_021201_create_logs_table::Migration),
            Box::new(m20250727_150336_add_run_model::Migration),
            Box::new(m20250811_084647_add_organizations_table::Migration),
            Box::new(m20250811_084822_add_organization_users_table::Migration),
            Box::new(m20250811_085101_projects_table::Migration),
            Box::new(m20250811_090444_branches_table::Migration),
            Box::new(m20250813_071440_add_project_id_to_threads_table::Migration),
            Box::new(m20250813_071500_add_project_id_to_secrets_table::Migration),
            Box::new(m20250813_071600_add_project_id_to_runs_table::Migration),
            Box::new(m20250819_020551_add_project_id_to_api_keys_table::Migration),
            Box::new(m20250819_084109_fix_root_replay_ref_type::Migration),
            Box::new(m20250902_073902_add_active_branch_id_to_projects_table::Migration),
            Box::new(m20250902_080016_add_branch_id_to_runs_table::Migration),
            Box::new(m20250902_080217_add_project_branch_id_to_run_index_runs_table::Migration),
            Box::new(m20250923_064746_change_organization_to_workspace::Migration),
            Box::new(m20250924_015621_create_project_repos_and_update_projects::Migration),
            Box::new(m20250929_081920_create_git_namespaces_table::Migration),
            Box::new(m20251009_020233_add_run_variables_and_output::Migration),
        ]
    }
}
