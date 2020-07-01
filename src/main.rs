extern crate env_logger;
extern crate futures;
extern crate hyper;
#[macro_use]
extern crate log;
extern crate serde_json;

use std::collections::HashMap;
use std::error::Error;
use std::hint::unreachable_unchecked;
use std::io;
use serde_json::{json, Value};

use futures::future::{Future, FutureResult};
use futures::Stream;
use hyper::{Chunk, StatusCode};
use hyper::client::FutureResponse;
use hyper::header::{ContentLength, ContentType};
use hyper::Method::{Get, Post};
use hyper::server::{Request, Response, Service};
use std::panic::resume_unwind;

struct Microservice;

struct NewMessage {
    username: String,
    message: String,
}

struct TimeRange {
    before: Option<i64>,
    after: Option<i64>,
}

impl Service for Microservice {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = Box<dyn Future<Item=Self::Response, Error=Self::Error>>;

    fn call(&self, request: Request) -> Self::Future {
        match (request.method(), request.path()) {
            (&Post, "/") => {
                let future = request
                    .body()
                    .concat2()
                    .and_then(parse_form)
                    .and_then(write_to_db)
                    .then(make_post_response);
                Box::new(future)
            }
            (&Get, "/") => {
                let time_range = match request.query() {
                    Some(query) => parse_query(query),
                    None => Ok(TimeRange {
                        before: None,
                        after: None
                    }),
                };
                let response = match time_range {
                    Ok(time_range) => make_get_response(query_db(time_range)),
                    Err(error) => make_error_response(&error)
                };
                Box::new(response)
            }
            _ => Box::new(futures::future::ok(
                Response::new().with_status(StatusCode::NotFound)
            ))
        }
    }
}

fn parse_query(query: &str) -> Result<TimeRange, String> {
    let args = url::form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect::<HashMap<String, String>>();

    let before = args.get("before").map(|value| value.parse::<i64>());
    if let Some(ref result) = before {
        if let Err(ref err) = *result {
            return Err(format!("Error parsing 'before': {}", err));
        }
    }

    let after = args.get("after").map(|value| value.parse::<i64>());
    if let Some(ref result) = after {
        if let Err(ref err) = *result {
            return Err(format!("Error parsing 'after': {}", err));
        }
    }

    Ok(TimeRange{
        before: before.map(|b| b.unwrap()),
        after: after.map(|a| a.unwrap())
    })
}

fn parse_form(form_chunk: Chunk) -> FutureResult<NewMessage, hyper::Error> {
    let mut form = url::form_urlencoded::parse(form_chunk.as_ref())
        .into_owned()
        .collect::<HashMap<String, String>>();

    if let Some(message) = form.remove("message") {
        let username = form.remove("username").unwrap_or(String::from("unknown"));
        futures::future::ok(NewMessage{
            username,
            message
        })
    } else {
        futures::future::err(hyper::Error::from(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Missing field 'message"
        )))
    }
}

fn write_to_db(entry: NewMessage) -> FutureResult<i64, hyper::Error> {
    futures::future::ok(0)
}

fn make_post_response(
    result: Result<i64, hyper::Error>,
) -> FutureResult<hyper::Response, hyper::Error> {
    match result {
        Ok(timestamp) => {
            let payload = json!({"timestamp": timestamp}).to_string();
            let response = Response::new()
                .with_header(ContentLength(payload.len() as u64))
                .with_header(ContentType::json())
                .with_body(payload);
            debug!("{:?}", response);
            futures::future::ok(response)
        }
        Err(error) => make_error_response(error.description())
    }
}

fn make_get_response(messages: Option<Vec<Message>>,) -> FutureResult<hyper::Response, hyper::Error> {
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


fn make_error_response(error_message: &str) -> FutureResult<hyper::Response, hyper::Error> {
    let payload = json!({"error": error_message}).to_string();
    let response = Response::new()
        .with_header(ContentLength(payload.len() as u64))
        .with_header(ContentType::json())
        .with_body(payload);
    debug!("{:?}", response);
    futures::future::ok(response)
}



fn main() {
    env_logger::init();
    let address = "127.0.0.1:5000".parse().unwrap();
    let server = hyper::server::Http::new()
        .bind(&address, || Ok(Microservice {}))
        .unwrap();
    info!("Running microservice at {}", address);
    server.run().unwrap();
}
