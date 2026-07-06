//! Toast Analytics Metrics API: async, batched aggregated sales reporting.
//!
//! Flow: `POST /era/v1/metrics` with the restaurant ids + business-date window
//! returns a `reportRequestGuid`; `GET /era/v1/metrics/{guid}` returns the data
//! once generated (polled until ready). The response is an array of rows, one
//! per restaurant × business date × revenue center; we reduce it to a per-guid
//! sum. `parse_report_guid`, `parse_metrics_array`, and `reduce_check` are pure
//! and unit-tested without a network.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::super::config::ExternalSpec;
use super::super::source::ReconcileError;
use super::truncate_body;

/// Per-restaurant sums across the report's rows (dates + revenue centers).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RestaurantMetrics {
    pub net_sales: f64,
    pub gross_sales: f64,
    pub orders: f64,
    pub guests: f64,
    pub discounts: f64,
    pub voids: f64,
    pub refunds: f64,
}

impl RestaurantMetrics {
    /// Value for a config `metric`. Accepts both the raw Toast Analytics field
    /// (`netSalesAmount`) and the natural snake_case alias (`net_sales`), so the
    /// config can read like the Oxy measure side. Errors on an unknown name so a
    /// typo surfaces rather than silently reading zero.
    pub fn value(&self, metric: &str) -> Result<f64, ReconcileError> {
        match metric {
            "netSalesAmount" | "net_sales" => Ok(self.net_sales),
            "grossSalesAmount" | "gross_sales" => Ok(self.gross_sales),
            "ordersCount" | "order_count" | "orders" => Ok(self.orders),
            "guestCount" | "guest_count" | "guests" => Ok(self.guests),
            "discountAmount" | "discounts" | "discount" => Ok(self.discounts),
            "voidAmount" | "voids" | "void" => Ok(self.voids),
            "refundAmount" | "refunds" | "refund" => Ok(self.refunds),
            other => Err(ReconcileError::Fetch(format!(
                "unknown toast metric '{other}' (expected net_sales|gross_sales|order_count|\
                 guest_count|discounts|voids|refunds, or the raw netSalesAmount|grossSalesAmount|\
                 ordersCount|guestCount|discountAmount|voidAmount|refundAmount)"
            ))),
        }
    }
}

/// Create the report, poll until ready, and reduce to per-restaurant sums.
/// A rate-limit (HTTP 429) propagates as `ReconcileError::RateLimited`, which
/// degrades the affected checks with the rate-limited reason rather than
/// serving a stale prior report.
pub async fn fetch_report(
    http: &reqwest::Client,
    base_url: &str,
    token: &str,
    restaurant_ids: &[String],
    window: &[String; 2],
    timeout: Duration,
) -> Result<HashMap<String, RestaurantMetrics>, ReconcileError> {
    let guid = create_report(http, base_url, token, restaurant_ids, window).await?;
    let body = poll_report(http, base_url, token, &guid, timeout).await?;
    let map = parse_metrics_array(&body);
    log_report_summary(window, restaurant_ids, &map);
    Ok(map)
}

/// `POST /era/v1/metrics` → `reportRequestGuid`.
async fn create_report(
    http: &reqwest::Client,
    base_url: &str,
    token: &str,
    restaurant_ids: &[String],
    window: &[String; 2],
) -> Result<String, ReconcileError> {
    let (start, end) = business_dates(window)?;
    let url = format!("{base_url}/era/v1/metrics");
    // Per the OpenAPI schema (MetricsReportingDataRequest): dates are INTEGERS
    // (YYYYMMDD), and restaurantIds + excludedRestaurantIds are both REQUIRED
    // keys. An empty `restaurantIds` (with empty `excludedRestaurantIds`) means
    // the whole management group; a populated one narrows the report. `groupBy`
    // is optional and omitted — we sum all returned rows per restaurant anyway.
    let body = serde_json::json!({
        "startBusinessDate": start,
        "endBusinessDate": end,
        "restaurantIds": restaurant_ids,
        "excludedRestaurantIds": [],
    });
    let resp = http
        .post(&url)
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .map_err(|e| ReconcileError::Unreachable(format!("toast metrics create: {e}")))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| ReconcileError::Unreachable(format!("toast metrics create body: {e}")))?;
    if status.as_u16() == 429 {
        // Hard rate limit (10/hr). Surface it so the affected checks degrade
        // with the rate-limited reason rather than reporting a stale value.
        return Err(ReconcileError::RateLimited(
            "toast metrics create".to_string(),
        ));
    }
    if !status.is_success() {
        return Err(ReconcileError::Fetch(format!(
            "toast metrics create: HTTP {status} — {}",
            truncate_body(&text)
        )));
    }
    parse_report_guid(&text)
}

/// `GET /era/v1/metrics/{guid}` until the array is ready or `timeout` elapses.
/// ASSUMPTION (Toast docs don't pin the not-ready signal): a 202/404 or a
/// non-array 200 means "still generating"; a 200 array is the result.
async fn poll_report(
    http: &reqwest::Client,
    base_url: &str,
    token: &str,
    guid: &str,
    timeout: Duration,
) -> Result<Value, ReconcileError> {
    let url = format!("{base_url}/era/v1/metrics/{guid}");
    let start = Instant::now();
    let interval = Duration::from_millis(1000);
    loop {
        let resp = http
            .get(&url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| ReconcileError::Unreachable(format!("toast metrics fetch: {e}")))?;
        let status = resp.status();
        let pending = status.as_u16() == 202 || status.as_u16() == 404;
        if !pending {
            let text = resp
                .text()
                .await
                .map_err(|e| ReconcileError::Unreachable(format!("toast metrics body: {e}")))?;
            if status.as_u16() == 429 {
                // Hard rate limit (10/hr) hit while polling. Surface it like
                // `create_report` so the affected checks degrade with the
                // rate-limited reason rather than being mislabeled a fetch error.
                return Err(ReconcileError::RateLimited(
                    "toast metrics fetch".to_string(),
                ));
            }
            if !status.is_success() {
                return Err(ReconcileError::Fetch(format!(
                    "toast metrics fetch: HTTP {status} — {}",
                    truncate_body(&text)
                )));
            }
            let body: Value = serde_json::from_str(&text).map_err(|e| {
                ReconcileError::Fetch(format!(
                    "toast metrics json: {e} — body: {}",
                    truncate_body(&text)
                ))
            })?;
            if body.is_array() {
                return Ok(body);
            }
            // 200 but not the array yet — keep polling.
        }
        if start.elapsed() >= timeout {
            return Err(ReconcileError::Unreachable(format!(
                "toast metrics not ready after {timeout:?}"
            )));
        }
        tokio::time::sleep(interval).await;
    }
}

/// The create response is a single GUID — a bare JSON string or plain text.
fn parse_report_guid(text: &str) -> Result<String, ReconcileError> {
    if let Ok(s) = serde_json::from_str::<String>(text.trim())
        && !s.is_empty()
    {
        return Ok(s);
    }
    let t = text.trim().trim_matches('"').trim();
    if t.is_empty() {
        return Err(ReconcileError::Fetch(
            "toast metrics create: empty reportRequestGuid".to_string(),
        ));
    }
    Ok(t.to_string())
}

/// Reduce the report array to per-`restaurantGuid` sums across all rows.
pub fn parse_metrics_array(body: &Value) -> HashMap<String, RestaurantMetrics> {
    let mut map: HashMap<String, RestaurantMetrics> = HashMap::new();
    let Some(rows) = body.as_array() else {
        return map;
    };
    for row in rows {
        let Some(guid) = row.get("restaurantGuid").and_then(Value::as_str) else {
            continue;
        };
        let entry = map.entry(guid.to_string()).or_default();
        entry.net_sales += field(row, "netSalesAmount");
        entry.gross_sales += field(row, "grossSalesAmount");
        entry.orders += field(row, "ordersCount");
        entry.guests += field(row, "guestCount");
        entry.discounts += field(row, "discountAmount");
        entry.voids += field(row, "voidAmount");
        entry.refunds += field(row, "refundAmount");
    }
    map
}

fn field(row: &Value, name: &str) -> f64 {
    row.get(name).and_then(Value::as_f64).unwrap_or(0.0)
}

/// Diagnostic: what the report actually contained, so a "lower than the Toast
/// UI" mismatch can be traced to a restaurant subset vs a period/metric gap.
/// `requested` is empty when we omitted `restaurantIds` (whole management group).
fn log_report_summary(
    window: &[String; 2],
    requested: &[String],
    map: &HashMap<String, RestaurantMetrics>,
) {
    let net: f64 = map.values().map(|m| m.net_sales).sum();
    let gross: f64 = map.values().map(|m| m.gross_sales).sum();
    let orders: f64 = map.values().map(|m| m.orders).sum();
    let guests: f64 = map.values().map(|m| m.guests).sum();
    let discounts: f64 = map.values().map(|m| m.discounts).sum();
    let voids: f64 = map.values().map(|m| m.voids).sum();
    let refunds: f64 = map.values().map(|m| m.refunds).sum();
    let mut guids: Vec<&String> = map.keys().collect();
    guids.sort();
    tracing::info!(
        target: "health_eval",
        window = format!("{}..{}", window[0], window[1]),
        requested_restaurants = requested.len(),
        report_restaurants = map.len(),
        // The restaurant GUIDs Toast actually returned — compare against the
        // restaurants you see in the UI to find which the credentials can't see.
        restaurants = ?guids,
        net_sales = net,
        gross_sales = gross,
        orders = orders,
        // New fields: an all-zero sum here on a non-empty report means the raw
        // Toast field name is wrong (guestCount/discountAmount/voidAmount/
        // refundAmount) — the summed value confirms the name empirically.
        guests = guests,
        discounts = discounts,
        voids = voids,
        refunds = refunds,
        "toast metrics report"
    );
}

/// Sum `spec.metric` over the selected restaurants — all of them when
/// `spec.restaurants` is empty, otherwise just the listed GUIDs. A requested
/// restaurant absent from the report contributes 0 (no sales in the window).
pub fn reduce_check(
    map: &HashMap<String, RestaurantMetrics>,
    spec: &ExternalSpec,
) -> Result<f64, ReconcileError> {
    // Validate the metric name up front so a typo fails even on an empty report.
    RestaurantMetrics::default().value(&spec.metric)?;
    let mut sum = 0.0;
    if spec.restaurants.is_empty() {
        for m in map.values() {
            sum += m.value(&spec.metric)?;
        }
    } else {
        for guid in &spec.restaurants {
            if let Some(m) = map.get(guid) {
                sum += m.value(&spec.metric)?;
            }
        }
    }
    Ok(sum)
}

/// `[YYYY-MM-DD, YYYY-MM-DD]` → Toast's `YYYYMMDD` business dates as integers
/// (the schema types them as numbers).
fn business_dates(window: &[String; 2]) -> Result<(i64, i64), ReconcileError> {
    let parse = |s: &str| {
        s.replace('-', "")
            .parse::<i64>()
            .map_err(|e| ReconcileError::Fetch(format!("bad business date '{s}': {e}")))
    };
    Ok((parse(&window[0])?, parse(&window[1])?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(metric: &str, restaurants: &[&str]) -> ExternalSpec {
        ExternalSpec {
            source: "toast".to_string(),
            integration: None,
            metric: metric.to_string(),
            restaurants: restaurants.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn sample() -> Value {
        // Two restaurants, restaurant "a" split across two rows (dates / revenue
        // centers) to exercise summing.
        serde_json::json!([
            { "restaurantGuid": "a", "netSalesAmount": 100.0, "grossSalesAmount": 120.0, "ordersCount": 10, "guestCount": 12, "discountAmount": 8.0,  "voidAmount": 2.0, "refundAmount": 1.0 },
            { "restaurantGuid": "a", "netSalesAmount": 50.0,  "grossSalesAmount": 60.0,  "ordersCount": 5,  "guestCount": 6,  "discountAmount": 4.0,  "voidAmount": 1.0, "refundAmount": 0.0 },
            { "restaurantGuid": "b", "netSalesAmount": 200.0, "grossSalesAmount": 220.0, "ordersCount": 20, "guestCount": 25, "discountAmount": 15.0, "voidAmount": 3.0, "refundAmount": 5.0 }
        ])
    }

    #[test]
    fn parses_report_guid_quoted_or_plain() {
        assert_eq!(parse_report_guid("\"abc-123\"").unwrap(), "abc-123");
        assert_eq!(parse_report_guid("  abc-123  ").unwrap(), "abc-123");
        assert!(parse_report_guid("  ").is_err());
    }

    #[test]
    fn sums_rows_per_restaurant() {
        let map = parse_metrics_array(&sample());
        assert_eq!(map["a"].net_sales, 150.0);
        assert_eq!(map["a"].orders, 15.0);
        assert_eq!(map["b"].gross_sales, 220.0);
        // New metrics sum across a restaurant's rows too.
        assert_eq!(map["a"].guests, 18.0);
        assert_eq!(map["a"].discounts, 12.0);
        assert_eq!(map["a"].voids, 3.0);
        assert_eq!(map["b"].refunds, 5.0);
    }

    #[test]
    fn new_metrics_resolve_by_alias_and_raw_name() {
        let map = parse_metrics_array(&sample());
        // guests: raw guestCount == snake_case guest_count == plural guests.
        assert_eq!(reduce_check(&map, &spec("guest_count", &[])).unwrap(), 43.0);
        assert_eq!(
            reduce_check(&map, &spec("guestCount", &[])).unwrap(),
            reduce_check(&map, &spec("guests", &[])).unwrap()
        );
        assert_eq!(
            reduce_check(&map, &spec("discounts", &["a"])).unwrap(),
            12.0
        );
        assert_eq!(reduce_check(&map, &spec("voids", &[])).unwrap(), 6.0);
        assert_eq!(reduce_check(&map, &spec("refundAmount", &[])).unwrap(), 6.0);
    }

    #[test]
    fn reduce_all_restaurants_sums_everything() {
        let map = parse_metrics_array(&sample());
        let got = reduce_check(&map, &spec("netSalesAmount", &[])).unwrap();
        assert_eq!(got, 350.0); // 150 (a) + 200 (b)
    }

    #[test]
    fn metric_accepts_snake_case_alias() {
        let map = parse_metrics_array(&sample());
        // `net_sales` resolves the same as the raw `netSalesAmount`.
        assert_eq!(
            reduce_check(&map, &spec("net_sales", &[])).unwrap(),
            reduce_check(&map, &spec("netSalesAmount", &[])).unwrap()
        );
        assert_eq!(
            reduce_check(&map, &spec("order_count", &["a"])).unwrap(),
            15.0
        );
    }

    #[test]
    fn reduce_specific_restaurant() {
        let map = parse_metrics_array(&sample());
        let got = reduce_check(&map, &spec("ordersCount", &["a"])).unwrap();
        assert_eq!(got, 15.0);
    }

    #[test]
    fn reduce_missing_restaurant_contributes_zero() {
        let map = parse_metrics_array(&sample());
        let got = reduce_check(&map, &spec("netSalesAmount", &["zzz"])).unwrap();
        assert_eq!(got, 0.0);
    }

    #[test]
    fn reduce_unknown_metric_errors_even_on_empty_report() {
        let empty: HashMap<String, RestaurantMetrics> = HashMap::new();
        assert!(reduce_check(&empty, &spec("bogus", &[])).is_err());
    }

    #[test]
    fn business_dates_to_integers() {
        let (s, e) = business_dates(&["2026-06-17".to_string(), "2026-06-23".to_string()]).unwrap();
        assert_eq!((s, e), (20260617, 20260623));
    }
}
