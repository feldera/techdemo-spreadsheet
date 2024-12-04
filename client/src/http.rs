use std::ops::ControlFlow;
use std::sync::Arc;

use egui::mutex::Mutex;
use ehttp::{streaming, Request};
use log::error;

pub fn streaming_request(url: String, handle_data: Arc<dyn Fn(String) -> ControlFlow<()> + Send + Sync>) {
    let remainder = Arc::new(Mutex::new(String::new()));

    // Handle a chunk of data received from the server, this might not be a complete JSON object
    // so we need to store the remainder of the last chunk and append it to the next chunk
    let handle_chunk: Arc<dyn Fn(Vec<u8>) -> ControlFlow<()> + Send + Sync> = Arc::new(move |chunk: Vec<u8>| {
        if chunk.is_empty() {
            return ControlFlow::Break(());
        }
        let mut remainder = remainder.lock();

        let mut current_chunk = remainder.to_string();
        current_chunk.extend(String::from_utf8_lossy(chunk.as_slice()).chars());

        // For ndjson, needs to end with a newline, if not it's an incomplete chunk
        // store the last bit in the remainder
        if !current_chunk.ends_with('\n') {
            // split off the last chunk that doesn't end with a newline
            let (chunk_str, new_remainder) = match current_chunk.rfind('\n') {
                Some(idx) => current_chunk.split_at(idx + 1),
                None => {
                    *remainder = current_chunk;
                    return ControlFlow::Continue(());
                }
            };
            *remainder = new_remainder.to_string();
            current_chunk = chunk_str.to_string();
        }
        else {
            *remainder = String::new();
        }

        handle_data(current_chunk)
    });


    let request = Request::get(url.clone());
    streaming::fetch(request, move |result: ehttp::Result<streaming::Part>| {
        let part = match result {
            Ok(part) => part,
            Err(err) => {
                error!("an error occurred while streaming `{url}`: {err}");
                return ControlFlow::Break(());
            }
        };

        match part {
            streaming::Part::Response(response) => {
                if response.ok {
                    ControlFlow::Continue(())
                } else {
                    ControlFlow::Break(())
                }
            }
            streaming::Part::Chunk(chunk) => {
                handle_chunk(chunk)
            }
        }
    });
}