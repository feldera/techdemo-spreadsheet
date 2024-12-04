//! Helper functions for the Feldera API

use std::env::var;
use std::io;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use axum::http::StatusCode;
use axum::Json;
use dashmap::DashSet;
use futures::{StreamExt, TryStreamExt};
use log::{error, warn};
use reqwest::Client;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::broadcast::Sender;

use crate::stats::XlsError;

const PIPELINE_NAME: &str = "xls";
const FELDERA_HOST: LazyLock<String> =
    LazyLock::new(|| var("FELDERA_HOST").unwrap_or_else(|_| String::from("http://localhost:8080")));
static FELDERA_API_KEY: LazyLock<String> =
    LazyLock::new(|| var("FELDERA_API_KEY").unwrap_or_else(|_| String::new()));

pub(crate) async fn adhoc_query(sql: &str) -> Result<String, XlsError> {
    let url = format!("{}/v0/pipelines/{PIPELINE_NAME}/query", &*FELDERA_HOST);
    let client = Client::new();
    let response = client
        .get(url)
        .bearer_auth(&*FELDERA_API_KEY)
        .query(&[("sql", sql), ("format", "json")])
        .send()
        .await
        .map_err(XlsError::from)?;

    if !response.status().is_success() {
        return Err(XlsError::from(format!(
            "Failed to fetch data: HTTP {}: {:?}",
            response.status(),
            response.text().await.unwrap_or_else(|e| e.to_string())
        )));
    }

    let body = response.text().await.map_err(XlsError::from)?;

    Ok(body)
}

/// Parses feldera change format inside of json_data
///
/// `{"sequence_number": ...,"json_data":[{"delete": {...} },{"insert": {...} }]}`
#[derive(serde::Deserialize)]
#[allow(dead_code)]
enum Change {
    #[serde(rename = "insert")]
    Insert(Value),
    #[serde(rename = "delete")]
    Delete(Value),
}

/// Parses a record from the feldera change stream.
#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct Record {
    sequence_number: i64,
    json_data: Option<Vec<Change>>,
}

pub(crate) fn subscribe_change_stream(
    view_name: &str,
    capacity: usize,
) -> Sender<Result<String, XlsError>> {
    let (tx, _) = tokio::sync::broadcast::channel(capacity);
    let subscribe = tx.clone();
    let client = Client::new();
    let url = format!(
        "{}/v0/pipelines/{PIPELINE_NAME}/egress/{view_name}",
        &*FELDERA_HOST
    );
    let view = String::from(view_name);

    tokio::spawn(async move {
        loop {
            let response = client
                .post(url.clone())
                .bearer_auth(&*FELDERA_API_KEY)
                .header("Content-Type", "application/json")
                .query(&[
                    ("format", "json"),
                    ("backpressure", "false"),
                    ("array", "false"),
                ])
                .send()
                .await;

            match response {
                Ok(resp) if resp.status().is_success() => {
                    let stream = resp
                        .bytes_stream()
                        .map_err(|e| io::Error::new(io::ErrorKind::Other, e));
                    let reader = tokio_util::io::StreamReader::new(stream);
                    let mut decoder = tokio_util::codec::FramedRead::new(
                        reader,
                        tokio_util::codec::LinesCodec::new(),
                    );

                    while let Some(line) = decoder.next().await {
                        match line {
                            Ok(line) => {
                                //log::debug!("Received change: {line}");
                                match serde_json::from_str::<Record>(&line) {
                                    Ok(record) => {
                                        // walk record.json_data in reverse and return first `insert`
                                        'inner: for change in
                                            record.json_data.unwrap_or_else(|| vec![]).iter().rev()
                                        {
                                            if let Change::Insert(value) = change {
                                                let mut value_str = value.to_string();
                                                value_str.push('\n');
                                                //log::debug!("broadcasting change: {value_str}");
                                                if tx.send(Ok(value_str)).is_err() {
                                                    // A send operation can only fail if there are no active receivers,
                                                    // implying that the message could never be received.
                                                    // The error contains the message being sent as a payload so it can be recovered.
                                                    break 'inner;
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        error!("Failed to parse change record from {view}: {}", e);
                                        break;
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Failed to decode line from {view}: {:?}", e);
                                let _ = tx.send(Err(XlsError::from(e)));
                                break;
                            }
                        }
                    }
                }
                _ => {
                    error!("Failed to fetch change stream at {url}: {:?}", response);
                    let _ = tx.send(Err(XlsError::from("Failed to fetch change stream")));
                }
            }

            warn!("Lost connection to change stream at {url}, wait 10 seconds before retrying to get changes again");
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    });

    subscribe
}

pub(crate) async fn insert<T: Serialize>(table_name: &str, data: T) -> (StatusCode, Json<Value>) {
    let client = Client::new();
    let url = format!(
        "{}/v0/pipelines/{PIPELINE_NAME}/ingress/{table_name}",
        &*FELDERA_HOST
    );

    let response = client
        .post(url.clone())
        .bearer_auth(&*FELDERA_API_KEY)
        .header("Content-Type", "application/json")
        .query(&[("format", "json"), ("update_format", "raw")])
        .json(&data)
        .send()
        .await;

    match response {
        Ok(resp) if resp.status().is_success() => {
            (StatusCode::OK, Json(serde_json::json!({"success": true})))
        }
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to update cell"})),
        ),
    }
}

#[derive(serde::Deserialize, Debug)]
struct ApiLimitRecord {
    ip: String,
}

pub(crate) fn api_limit_table() -> Arc<DashSet<String>> {
    let ds = Arc::new(DashSet::new());
    let ds_clone = ds.clone();
    let client = Client::new();
    let url = format!(
        "{}/v0/pipelines/{PIPELINE_NAME}/egress/api_limit_reached",
        &*FELDERA_HOST
    );

    tokio::spawn(async move {
        loop {
            ds.clear();
            let snapshot = adhoc_query("SELECT * FROM api_limit_reached")
                .await
                .unwrap_or_else(|e| {
                    error!("Failed to fetch initial api_limit data: {}", e);
                    String::new()
                });
            for line in snapshot.lines() {
                match serde_json::from_str::<ApiLimitRecord>(line) {
                    Ok(record) => {
                        log::debug!("Initial api limit: {record:?}");
                        ds.insert(record.ip);
                    }
                    Err(e) => {
                        error!("Failed to parse ApiLimitRecord: {}", e);
                    }
                }
            }

            let response = client
                .post(url.clone())
                .bearer_auth(&*FELDERA_API_KEY)
                .header("Content-Type", "application/json")
                .query(&[
                    ("format", "json"),
                    ("backpressure", "true"),
                    ("array", "false"),
                ])
                .send()
                .await;

            match response {
                Ok(resp) if resp.status().is_success() => {
                    let stream = resp
                        .bytes_stream()
                        .map_err(|e| io::Error::new(io::ErrorKind::Other, e));
                    let reader = tokio_util::io::StreamReader::new(stream);
                    let mut decoder = tokio_util::codec::FramedRead::new(
                        reader,
                        tokio_util::codec::LinesCodec::new(),
                    );

                    while let Some(line) = decoder.next().await {
                        match line {
                            Ok(line) => {
                                match serde_json::from_str::<Record>(&line) {
                                    Ok(record) => {
                                        // walk record.json_data in reverse and return first `insert`
                                        for change in
                                            record.json_data.unwrap_or_else(|| vec![]).into_iter()
                                        {
                                            match change {
                                                Change::Insert(value) => {
                                                    let record =
                                                        serde_json::from_value::<ApiLimitRecord>(
                                                            value,
                                                        );
                                                    match record {
                                                        Ok(record) => {
                                                            log::debug!(
                                                                "Received api limit for: {record:?}"
                                                            );
                                                            ds.insert(record.ip);
                                                        }
                                                        Err(e) => {
                                                            error!("Failed to parse ApiLimitRecord: {}", e);
                                                        }
                                                    }
                                                }
                                                Change::Delete(value) => {
                                                    let record =
                                                        serde_json::from_value::<ApiLimitRecord>(
                                                            value,
                                                        );
                                                    match record {
                                                        Ok(record) => {
                                                            log::debug!(
                                                                "Received api limit removal for: {record:?}"
                                                            );
                                                            ds.remove(&record.ip);
                                                        }
                                                        Err(e) => {
                                                            error!("Failed to parse ApiLimitRecord: {}", e);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        error!(
                                                "Failed to parse change record from api_limit_reached: {}",
                                                e
                                            );
                                        break;
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Failed to decode line from api_limit_reached: {:?}", e);
                                break;
                            }
                        }
                    }
                }
                _ => {
                    error!("Failed to fetch change stream at {url}: {:?}", response);
                }
            }

            warn!("Lost connection to change stream at {url}, wait 10 seconds before retrying to get changes again");
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    });

    ds_clone
}
