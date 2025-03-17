use oxy::cli::{RunArgs, RunOptions, RunResult};
use pyo3::prelude::*;

fn tokio() -> &'static tokio::runtime::Runtime {
    use std::sync::OnceLock;
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| tokio::runtime::Runtime::new().unwrap())
}

#[pyfunction]
#[pyo3(signature = (file, options=None))]
fn run(file: String, options: Option<RunOptions>) -> PyResult<RunResult> {
    let runtime = tokio();
    let task = runtime.spawn(async move {
        oxy::cli::handle_run_command(RunArgs::from(file, options))
            .await
            .unwrap()
    });
    let rs = runtime.block_on(task).unwrap();
    Ok(rs)
}

#[pyfunction]
#[pyo3(signature = (file, options=None))]
async fn run_async(file: String, options: Option<RunOptions>) -> PyResult<RunResult> {
    let rs = tokio()
        .spawn(async move {
            oxy::cli::handle_run_command(RunArgs::from(file, options))
                .await
                .unwrap()
        })
        .await
        .unwrap();
    Ok(rs)
}

#[pymodule]
fn oxy_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(run, m)?)?;
    m.add_function(wrap_pyfunction!(run_async, m)?)?;
    Ok(())
}
