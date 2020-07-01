use chrono::prelude::*;
use futures::future::FutureResult;
use hyper::header::{ContentLength, ContentType};
use hyper::server::Response;
use hyper::StatusCode;
use maud::html;

use crate::models::{Message, NewMessage};

pub fn post_response(
    result: Result<i64, hyper::Error>,
) -> FutureResult<hyper::Response, hyper::Error> {
    match result {
        Ok(timestamp) => {
            let payload = json!({ "timestamp": timestamp }).to_string();
            let response = Response::new()
                .with_header(ContentLength(payload.len() as u64))
                .with_header(ContentType::json())
                .with_body(payload);
            debug!("{:?}", response);
            futures::future::ok(response)
        }
        Err(error) => error_response(&error.to_string()),
    }
}

pub fn get_response(messages: Option<Vec<Message>>) -> FutureResult<hyper::Response, hyper::Error> {
    let response = match messages {
        Some(messages) => {
            let body = render_page(messages);
            Response::new()
                .with_header(ContentLength(body.len() as u64))
                .with_body(body)
        }
        None => Response::new().with_status(StatusCode::InternalServerError),
    };
    debug!("{:?}", response);
    futures::future::ok(response)
}

pub fn error_response(error_message: &str) -> FutureResult<hyper::Response, hyper::Error> {
    let payload = json!({ "error": error_message }).to_string();
    let response = Response::new()
        .with_header(ContentLength(payload.len() as u64))
        .with_header(ContentType::json())
        .with_body(payload);
    debug!("{:?}", response);
    futures::future::ok(response)
}

pub fn render_page(messages: Vec<Message>) -> String {
    (html! {
        head {
            title { "Message Service" }
            style { "body { font-family: monospace }" }
        }
        body {
            @if !messages.is_empty() {
                ul {
                        @for message in &messages {
                            li {
                                (message.username) " (" (format_date(message.timestamp)) "): " (message.message)
                            }
                        }
                    }
            } @else {
                    h1 { "No messages found" }
            }
        }
    })
        .into_string()
}

fn format_date(time_stamp: i64) -> String {
    let naive_datetime = NaiveDateTime::from_timestamp(time_stamp, 0);
    naive_datetime.to_string()
}
