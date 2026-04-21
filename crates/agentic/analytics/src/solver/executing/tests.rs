use super::*;
use crate::procedure::{ProcedureOutput, ProcedureStepResult};

fn make_step(
    name: &str,
    cols: Vec<&str>,
    rows: Vec<Vec<serde_json::Value>>,
) -> ProcedureStepResult {
    let row_count = rows.len() as u64;
    ProcedureStepResult {
        step_name: name.to_string(),
        columns: cols.into_iter().map(String::from).collect(),
        rows,
        truncated: false,
        total_row_count: row_count,
    }
}

#[test]
fn empty_steps_returns_placeholder() {
    let result = procedure_output_to_result(ProcedureOutput { steps: vec![] });
    assert_eq!(result.results.len(), 1);
    assert_eq!(result.results[0].data.columns, vec!["result"]);
}

#[test]
fn single_step_produces_single_result_set() {
    let result = procedure_output_to_result(ProcedureOutput {
        steps: vec![make_step(
            "q1",
            vec!["a", "b"],
            vec![vec![serde_json::json!("x"), serde_json::json!(2)]],
        )],
    });
    assert_eq!(result.results.len(), 1);
    assert!(!result.is_multi());
    assert_eq!(result.results[0].data.columns, vec!["a", "b"]);
    assert_eq!(result.results[0].data.total_row_count, 1);
}

#[test]
fn multiple_steps_produce_multi_result() {
    let result = procedure_output_to_result(ProcedureOutput {
        steps: vec![
            make_step("q1", vec!["x"], vec![vec![serde_json::json!(1)]]),
            make_step("q2", vec!["y"], vec![vec![serde_json::json!(2)]]),
            make_step("q3", vec!["z"], vec![vec![serde_json::json!(3)]]),
        ],
    });
    assert_eq!(result.results.len(), 3);
    assert!(result.is_multi());
}

// Numeric JSON values from to_typed_rows must arrive as CellValue::Number
// so that the chart renderer receives proper JSON numbers, not strings.
#[test]
fn numeric_json_cells_become_number_cell_values() {
    use agentic_core::result::CellValue;

    let result = procedure_output_to_result(ProcedureOutput {
        steps: vec![make_step(
            "revenue_by_region",
            vec!["region", "total_revenue"],
            vec![
                vec![serde_json::json!("North"), serde_json::json!(42000.0)],
                vec![serde_json::json!("South"), serde_json::json!(31500.5)],
                vec![serde_json::json!("West"), serde_json::json!(0)],
            ],
        )],
    });
    let rows = &result.results[0].data.rows;
    // String values stay as text.
    assert!(matches!(&rows[0].0[0], CellValue::Text(s) if s == "North"));
    // JSON numbers become CellValue::Number.
    assert!(
        matches!(rows[0].0[1], CellValue::Number(n) if n == 42000.0),
        "expected Number(42000.0), got {:?}",
        rows[0].0[1]
    );
    assert!(
        matches!(rows[1].0[1], CellValue::Number(n) if n == 31500.5),
        "expected Number(31500.5), got {:?}",
        rows[1].0[1]
    );
    assert!(
        matches!(rows[2].0[1], CellValue::Number(n) if n == 0.0),
        "expected Number(0.0), got {:?}",
        rows[2].0[1]
    );
}
