#[cfg(test)]
pub mod list_tables {
    use assert_cmd::Command;

    #[test]
    fn list_tables_ok() {
        let mut binding = Command::cargo_bin("onyx").unwrap();
        let cmd = binding.current_dir("examples").arg("list-tables");
        let result = cmd.assert().success();
        let output = result.get_output();
        let stdout = String::from_utf8(output.stdout.clone()).unwrap();
        assert!(stdout.contains("df-warehouse-prod.dbt_prod_core.fct_typeform_leads"));
        assert!(stdout.contains("df-warehouse-prod.dbt_prod_core.fct_user_events"));
    }
}
