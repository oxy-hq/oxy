use ::oxy::theme::StyledText;
use base64::Engine;
use clap::Parser;
use headless_chrome::{Browser, browser::tab::Tab};
use oxy_shared::errors::OxyError;
use std::collections::HashMap;
use std::sync::Arc;
use std::{fs, path::PathBuf, time::Duration};
use uuid::Uuid;

const CHART_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(30);
const CHART_EXPORT_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL_FAST: Duration = Duration::from_millis(100);
const POLL_INTERVAL_SLOW: Duration = Duration::from_millis(500);

#[derive(Parser, Debug)]
pub struct ExportChartArgs {
    /// Path to the app file
    #[clap(long, short = 'a', value_name = "PATH")]
    pub app_path: String,

    /// Output directory for PNG files
    #[clap(long, short = 'o', value_name = "PATH")]
    pub output: PathBuf,
}

/// Export charts to a directory without CLI output.
/// Returns a map of chart_index -> file_name for successfully exported charts.
/// This is the core logic reused by both the CLI command and the API endpoint.
/// Uses spawn_blocking to avoid blocking the async runtime (the headless browser
/// needs the server to remain responsive to serve the app page).
pub async fn export_charts_to_dir(
    app_path: &str,
    output_dir: &std::path::Path,
) -> Result<HashMap<i64, String>, OxyError> {
    let url = build_app_url(app_path);
    let output_dir = output_dir.to_path_buf();

    tokio::task::spawn_blocking(move || {
        fs::create_dir_all(&output_dir).map_err(|e| {
            OxyError::RuntimeError(format!("Failed to create output directory: {}", e))
        })?;

        let (_browser, tab) = launch_browser_and_navigate(&url)?;
        let chart_indexes = discover_charts(&tab)?;
        let exported_charts = export_all_charts(&tab, &chart_indexes, &output_dir)?;

        let result: HashMap<i64, String> = exported_charts
            .into_iter()
            .filter_map(|(index, path)| {
                path.map(|p| {
                    (
                        index,
                        p.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                    )
                })
            })
            .collect();

        Ok(result)
    })
    .await
    .map_err(|e| OxyError::RuntimeError(format!("Chart export task panicked: {}", e)))?
}

pub async fn handle_export_chart_command(
    args: ExportChartArgs,
) -> Result<HashMap<i64, String>, OxyError> {
    let app_path = args.app_path.trim();
    let url = build_app_url(app_path);
    print_header(app_path, &url, &args.output);

    fs::create_dir_all(&args.output)
        .map_err(|e| OxyError::RuntimeError(format!("Failed to create output directory: {}", e)))?;

    let (_browser, tab) = launch_browser_and_navigate(&url)?;
    let chart_indexes = discover_charts(&tab)?;

    println!(
        "{}",
        format!(
            "   Found {} chart(s) with indexes {:?}, exporting sequentially...",
            chart_indexes.len(),
            chart_indexes
        )
        .text()
    );

    let exported_charts = export_all_charts(&tab, &chart_indexes, &args.output)?;
    print_summary(&exported_charts, &args.output)?;

    // Build result map with only successful exports (file names only)
    let result: HashMap<i64, String> = exported_charts
        .into_iter()
        .filter_map(|(index, path)| {
            path.map(|p| {
                (
                    index,
                    p.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                )
            })
        })
        .collect();

    println!("Export result: {:?}", result);

    Ok(result)
}

fn print_header(app_path: &str, url: &str, output: &PathBuf) {
    println!("{}", "📊 Exporting charts...".info());
    println!("   App path: {}", app_path.secondary());
    println!("   URL: {}", url.secondary());
    println!(
        "   Output directory: {}",
        output.display().to_string().secondary()
    );
}

fn build_app_url(app_path: &str) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(app_path);
    // Use internal port
    format!("http://localhost:3001/apps/{}", encoded)
}

fn build_export_url(url: &str) -> String {
    if url.contains('?') {
        format!("{}&export=true", url)
    } else {
        format!("{}?export=true", url)
    }
}

fn launch_browser_and_navigate(url: &str) -> Result<(Browser, Arc<Tab>), OxyError> {
    println!("{}", "   Launching headless browser...".text());
    println!(
        "{}",
        "   (Using system Chromium from CHROME environment variable)".text()
    );

    let browser = Browser::new(
        headless_chrome::LaunchOptions::default_builder()
            .headless(true)
            .sandbox(false) // Required when running as root in Docker
            .build()
            .map_err(|e| OxyError::RuntimeError(format!("Failed to configure browser: {}", e)))?,
    )
    .map_err(|e| OxyError::RuntimeError(format!("Failed to launch browser: {}", e)))?;

    let tab = browser
        .new_tab()
        .map_err(|e| OxyError::RuntimeError(format!("Failed to create browser tab: {}", e)))?;

    let export_url = build_export_url(url);
    println!("{}", "   Navigating to app page with export mode...".text());
    tab.navigate_to(&export_url)
        .map_err(|e| OxyError::RuntimeError(format!("Failed to navigate to app page: {}", e)))?;

    println!("{}", "   Waiting for page to load...".text());
    tab.wait_until_navigated()
        .map_err(|e| OxyError::RuntimeError(format!("Page navigation timeout: {}", e)))?;

    Ok((browser, tab))
}

fn discover_charts(tab: &Arc<Tab>) -> Result<Vec<i64>, OxyError> {
    println!("{}", "   Waiting for app to load...".text());

    // First, wait for the app preview container to appear (indicates React has rendered)
    let wait_for_app = r#"!!document.querySelector('[data-testid="app-preview"]')"#;
    let app_wait_start = std::time::Instant::now();
    let app_ready_timeout = Duration::from_secs(15);

    while app_wait_start.elapsed() < app_ready_timeout {
        if let Ok(result) = tab.evaluate(wait_for_app, false) {
            if result.value.and_then(|v| v.as_bool()).unwrap_or(false) {
                println!("{}", "   App preview container found".text());
                break;
            }
        }

        // Check for error states that indicate the page won't load
        let error_check = r#"
            (function() {
                const body = document.body?.innerText || '';
                const url = window.location.href;
                if (url.includes('/login') || url.includes('/auth')) return 'auth_required';
                if (body.includes('Project not found')) return 'project_not_found';
                if (body.includes('Failed to load')) return 'load_failed';
                if (body.includes('Something went wrong')) return 'error';
                if (body.includes('Sign in') || body.includes('Log in')) return 'auth_required';
                return null;
            })()
        "#;
        if let Ok(result) = tab.evaluate(error_check, false) {
            if let Some(val) = result.value {
                if let Some(error) = val.as_str() {
                    return Err(OxyError::RuntimeError(format!(
                        "Page failed to load: {}. Make sure the web app is running and configured correctly.",
                        error
                    )));
                }
            }
        }

        std::thread::sleep(POLL_INTERVAL_SLOW);
    }

    println!("{}", "   Waiting for charts to render...".text());

    let find_charts_js = r#"
        JSON.stringify(
            Array.from(document.querySelectorAll('.chart-wrapper'))
                .map(el => parseInt(el.getAttribute('data-chart-index'), 10))
                .filter(idx => !isNaN(idx))
        )
    "#;

    let start_time = std::time::Instant::now();
    while start_time.elapsed() < CHART_DISCOVERY_TIMEOUT {
        if let Some(indexes) = try_get_chart_indexes(tab, find_charts_js) {
            if !indexes.is_empty() {
                return Ok(indexes);
            }
        }
        std::thread::sleep(POLL_INTERVAL_SLOW);
    }

    Err(OxyError::RuntimeError(
        "No charts found on the page after 30s. Make sure the page has loaded correctly."
            .to_string(),
    ))
}

fn try_get_chart_indexes(tab: &Arc<Tab>, js: &str) -> Option<Vec<i64>> {
    tab.evaluate(js, false)
        .ok()
        .and_then(|r| r.value)
        .and_then(|v| v.as_str().map(String::from))
        .and_then(|s| serde_json::from_str(&s).ok())
}

fn export_all_charts(
    tab: &Arc<Tab>,
    indexes: &[i64],
    output_dir: &PathBuf,
) -> Result<Vec<(i64, Option<PathBuf>)>, OxyError> {
    let total = indexes.len();
    let mut results = Vec::with_capacity(total);

    for (i, &index) in indexes.iter().enumerate() {
        println!(
            "{}",
            format!(
                "   Exporting chart {} of {} (index: {})...",
                i + 1,
                total,
                index
            )
            .text()
        );
        let result = export_single_chart(tab, index, output_dir)?;
        results.push((index, result));
    }

    results.sort_by_key(|(index, _)| *index);
    Ok(results)
}

fn export_single_chart(
    tab: &Arc<Tab>,
    index: i64,
    output_dir: &PathBuf,
) -> Result<Option<PathBuf>, OxyError> {
    if !click_export_button(tab, index)? {
        println!(
            "{}",
            format!("   ⚠ Could not find export button for chart {}", index).text()
        );
        return Ok(None);
    }

    match wait_for_chart_data(tab, index) {
        Some((name, bytes)) => {
            let uuid = Uuid::new_v4();
            let output_file = output_dir.join(format!("{}-{}-{}.png", name, index, uuid));
            fs::write(&output_file, bytes).map_err(|e| {
                OxyError::RuntimeError(format!("Failed to write output file: {}", e))
            })?;
            println!(
                "{}",
                format!(
                    "   ✓ Exported: {}",
                    output_file
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                )
                .text()
            );
            Ok(Some(output_file))
        }
        None => {
            println!(
                "{}",
                format!("   ⚠ Timeout waiting for chart {} export (15s)", index).text()
            );
            Ok(None)
        }
    }
}

fn click_export_button(tab: &Arc<Tab>, index: i64) -> Result<bool, OxyError> {
    let click_js = format!(
        r#"(function() {{
            const button = document.querySelector('.chart-export-trigger-{}');
            if (button) {{ button.click(); return true; }}
            return false;
        }})()"#,
        index
    );

    let start = std::time::Instant::now();
    while start.elapsed() < CHART_EXPORT_TIMEOUT {
        let clicked = tab
            .evaluate(&click_js, false)
            .map_err(|e| {
                OxyError::RuntimeError(format!(
                    "Failed to click export button for chart {}: {}",
                    index, e
                ))
            })?
            .value
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if clicked {
            return Ok(true);
        }
        std::thread::sleep(POLL_INTERVAL_FAST);
    }

    Ok(false)
}

fn wait_for_chart_data(tab: &Arc<Tab>, index: i64) -> Option<(String, Vec<u8>)> {
    let selector = format!(".chart-export-result-{}[data-ready='true']", index);
    let start = std::time::Instant::now();

    while start.elapsed() < CHART_EXPORT_TIMEOUT {
        if let Some(data) = try_extract_chart_data(tab, &selector) {
            return Some(data);
        }
        std::thread::sleep(POLL_INTERVAL_FAST);
    }
    None
}

fn try_extract_chart_data(tab: &Arc<Tab>, selector: &str) -> Option<(String, Vec<u8>)> {
    let elements = tab.find_elements(selector).ok()?;
    let element = elements.first()?;
    let json_str = element.get_attribute_value("data-chart").ok()??;
    let chart_json: serde_json::Value = serde_json::from_str(&json_str).ok()?;

    let name = chart_json["name"].as_str().unwrap_or("chart").to_string();
    let image_data = chart_json["imageData"].as_str()?;
    let base64_data = image_data.strip_prefix("data:image/png;base64,")?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_data.trim())
        .ok()?;

    Some((name, bytes))
}

fn print_summary(results: &[(i64, Option<PathBuf>)], output_dir: &PathBuf) -> Result<(), OxyError> {
    let total = results.len();
    let successful = results.iter().filter(|(_, p)| p.is_some()).count();
    let failed = total - successful;

    if successful == 0 {
        return Err(OxyError::RuntimeError(
            "No charts were successfully exported".to_string(),
        ));
    }

    println!(
        "{}",
        format!(
            "✅ Successfully exported {} of {} chart(s) to: {}",
            successful,
            total,
            output_dir.display()
        )
        .success()
    );

    if failed > 0 {
        println!(
            "{}",
            format!("   ⚠ {} chart(s) failed to export", failed).text()
        );
    }

    println!("{}", "   Output files (sorted by index):".text());
    for (index, path) in results {
        match path {
            Some(p) => println!(
                "     {}: {}",
                index,
                p.file_name().unwrap_or_default().to_string_lossy()
            ),
            None => println!("     {}: <failed>", index),
        }
    }

    Ok(())
}
