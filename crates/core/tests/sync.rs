#[cfg(test)]
pub mod sync {
    use assert_cmd::Command;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn setup_command() -> Command {
        let mut cmd: Command = Command::cargo_bin("oxy").unwrap();
        cmd.current_dir("examples").arg("sync");
        cmd
    }

    fn file_contains(path: &Path, expected_content: &str) -> bool {
        match fs::read_to_string(path) {
            Ok(content) => content.contains(expected_content),
            Err(_) => false,
        }
    }

    #[test]
    fn test_sync_database_creates_expected_files() {
        let mut cmd = setup_command();
        cmd.arg("primary_database")
            .arg("-d")
            .arg("dbt_prod_metrics")
            .assert()
            .success();

        let examples_dir = PathBuf::from("examples");
        let sql_file = examples_dir.join(".databases/primary_database/dbt_prod_metrics.sql");
        let models_dir = examples_dir.join(".databases/primary_database/dbt_prod_metrics/models");

        assert!(sql_file.exists(), "SQL file was not created");
        assert!(models_dir.exists(), "Models directory was not created");

        assert!(
            file_contains(
                &sql_file,
                "CREATE TABLE `df-warehouse-prod.dbt_prod_metrics.monthly_active_organizations"
            ),
            "SQL file missing monthly_active_organizations table"
        );
        assert!(
            file_contains(
                &sql_file,
                "CREATE TABLE `df-warehouse-prod.dbt_prod_metrics.monthly_active_users"
            ),
            "SQL file missing monthly_active_users table"
        );
        assert!(
            file_contains(
                &sql_file,
                "CREATE TABLE `df-warehouse-prod.dbt_prod_metrics.monthly_active_organizations_with_warehouse"
            ),
            "SQL file missing monthly_active_organizations_with_warehouse table"
        );
        assert!(
            file_contains(
                &sql_file,
                "CREATE TABLE `df-warehouse-prod.dbt_prod_metrics.monthly_active_users_with_warehouse"
            ),
            "SQL file missing monthly_active_users_with_warehouse table"
        );

        let sem_files = [
            "monthly_active_organizations.sem.yml",
            "monthly_active_organizations_with_warehouse.sem.yml",
            "monthly_active_users.sem.yml",
            "monthly_active_users_with_warehouse.sem.yml",
        ];

        for sem_file in &sem_files {
            let sem_path = models_dir.join(sem_file);
            assert!(sem_path.exists(), "Missing .sem.yml file: {}", sem_file);

            let file_name_without_ext = sem_file.replace(".sem.yml", "");
            assert!(
                file_contains(
                    &sem_path,
                    &format!("table: dbt_prod_metrics.{}", file_name_without_ext)
                ),
                "{} does not contain expected table definition",
                sem_file
            );
        }
    }

    #[test]
    fn test_sync_database_fails_with_invalid_database() {
        let mut cmd = setup_command();
        let result = cmd.arg("non_existent_database").assert().failure();

        let output = String::from_utf8(result.get_output().stderr.clone()).unwrap();
        assert!(output.contains("Database 'non_existent_database' not found in config"));
    }

    #[test]
    fn test_sync_database_with_overwrite_flag() {
        let mut cmd = setup_command();
        cmd.arg("primary_database")
            .arg("-d")
            .arg("dbt_prod_metrics")
            .assert()
            .success();

        let examples_dir = PathBuf::from("examples");
        let sql_file = examples_dir.join(".databases/primary_database/dbt_prod_metrics.sql");
        let models_dir = examples_dir.join(".databases/primary_database/dbt_prod_metrics/models");

        assert!(sql_file.exists(), "SQL file was not created");
        assert!(models_dir.exists(), "Models directory was not created");

        let dummy_file = models_dir.join("dummy_test_file.sem.yml");
        std::fs::write(&dummy_file, "test content for deletion")
            .expect("Failed to write dummy file");
        assert!(dummy_file.exists(), "Failed to create dummy file for test");
        let mut cmd = setup_command();
        cmd.arg("primary_database")
            .arg("-d")
            .arg("dbt_prod_metrics")
            .arg("--overwrite")
            .assert()
            .success();

        assert!(
            !dummy_file.exists(),
            "Dummy file was not removed during overwrite"
        );

        let sql_file_overwrite =
            examples_dir.join(".databases/primary_database/dbt_prod_metrics.sql");
        let models_dir_overwrite =
            examples_dir.join(".databases/primary_database/dbt_prod_metrics/models");

        assert!(
            sql_file_overwrite.exists(),
            "SQL file was not recreated during overwrite"
        );
        assert!(
            models_dir_overwrite.exists(),
            "Models directory was not recreated during overwrite"
        );

        assert!(
            file_contains(
                &sql_file_overwrite,
                "CREATE TABLE `df-warehouse-prod.dbt_prod_metrics.monthly_active_organizations`"
            ),
            "SQL file missing monthly_active_organizations table after overwrite"
        );
    }

    #[test]
    fn test_sync_entire_database_with_overwrite_flag() {
        let mut cmd = setup_command();
        cmd.arg("local").assert().success();

        let examples_dir = PathBuf::from("examples");
        let database_dir = examples_dir.join(".databases/local");

        assert!(database_dir.exists(), "Database directory was not created");

        let dummy_file = database_dir.join("dummy_test_file.txt");
        std::fs::write(&dummy_file, "test content that should not be deleted")
            .expect("Failed to write dummy file");
        assert!(dummy_file.exists(), "Failed to create dummy file for test");

        let mut cmd = setup_command();
        cmd.arg("local").arg("--overwrite").assert().success();

        assert!(
            database_dir.exists(),
            "Database directory was not preserved during overwrite"
        );

        assert!(
            dummy_file.exists(),
            "Dummy file in parent directory was incorrectly deleted"
        );

        if dummy_file.exists() {
            let _ = fs::remove_file(&dummy_file);
        }
    }
}
