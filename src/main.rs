#![feature(proc_macro_hygiene)]

extern crate futures;
extern crate hyper;
extern crate maud;
extern crate url;

#[macro_use]
extern crate serde_json;

#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate diesel;

#[macro_use]
extern crate log;
extern crate env_logger;

use std::collections::HashMap;
use std::env;
use std::io;

use hyper::server::{Request, Response, Service};
use hyper::Method::{Get, Post};
use hyper::{Chunk, StatusCode};

use futures::future::{Future, FutureResult};
use futures::Stream;

use diesel::pg::PgConnection;
use diesel::prelude::*;

mod http_helpers;
mod models;
mod schema;

use http_helpers::*;
use models::{Message, NewMessage};

struct Microservice;

struct TimeRange {
    before: Option<i64>,
    after: Option<i64>,
}

const DEFAULT_DATABASE_URL: &'static str = "postgresql://postgres@localhost:5432/microservice";

impl Service for Microservice {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = Box<dyn Future<Item = Self::Response, Error = Self::Error>>;

    fn call(&self, request: Request) -> Self::Future {
        let db_connection = match connect_to_db() {
            Some(connection) => connection,
            None => {
                return Box::new(futures::future::ok(
                    Response::new().with_status(StatusCode::InternalServerError),
                ));
            }
        };

        match (request.method(), request.path()) {
            // Post a message as a user
            (&Post, "/") => {
                let future = request
                    .body()
                    .concat2()
                    .and_then(parse_form)
                    .and_then(move |new_message| write_to_db(new_message, &db_connection))
                    .then(post_response);
                Box::new(future)
            }
            // Get all messages - optionally between a time range
            (&Get, "/") => {
                let time_range = match request.query() {
                    Some(query) => parse_timestamp(query),
                    None => Ok(TimeRange {
                        before: None,
                        after: None,
                    }),
                };
                let response = match time_range {
                    Ok(time_range) => {
                        get_response(query_all_messages_by_time(time_range, &db_connection))
                    }
                    Err(error) => error_response(&error),
                };
                Box::new(response)
            }
            // Get all messages for a user
            (&Get, "/user") => {
                let response = match request.query() {
                    Some(query) => {
                        let user = parse_username(query);
                        match user {
                            Ok(username) => {
                                let user_messages =
                                    query_messages_by_user(&username, &db_connection);
                                get_response(user_messages)
                            }
                            Err(err) => error_response(&err),
                        }
                    }
                    None => futures::future::ok(Response::new().with_status(StatusCode::NotFound)),
                };

                Box::new(response)
            }
            _ => Box::new(futures::future::ok(
                Response::new().with_status(StatusCode::NotFound),
            )),
        }
    }
}

fn query_all_messages_by_time(
    time_range: TimeRange,
    db_connection: &PgConnection,
) -> Option<Vec<Message>> {
    use schema::messages;
    let TimeRange { before, after } = time_range;
    let query_result = match (before, after) {
        (Some(before), Some(after)) => messages::table
            .filter(messages::timestamp.lt(before as i64))
            .filter(messages::timestamp.gt(after as i64))
            .load::<Message>(db_connection),
        (Some(before), _) => messages::table
            .filter(messages::timestamp.lt(before as i64))
            .load::<Message>(db_connection),
        (_, Some(after)) => messages::table
            .filter(messages::timestamp.gt(after as i64))
            .load::<Message>(db_connection),
        _ => messages::table.load::<Message>(db_connection),
    };
    match query_result {
        Ok(result) => Some(result),
        Err(error) => {
            error!("Error querying DB: {}", error);
            None
        }
    }
}

fn query_messages_by_user(user_name: &str, db_connection: &PgConnection) -> Option<Vec<Message>> {
    use schema::messages;
    let query_result = messages::table
        .filter(messages::username.eq(user_name.to_string()))
        .load::<Message>(db_connection);

    match query_result {
        Ok(result) => Some(result),
        Err(error) => {
            error!("Error querying DB: {}", error);
            None
        }
    }
}

fn connect_to_db() -> Option<PgConnection> {
    let database_url = env::var("DATABASE_URL").unwrap_or(String::from(DEFAULT_DATABASE_URL));
    match PgConnection::establish(&database_url) {
        Ok(connection) => Some(connection),
        Err(error) => {
            error!("Error connecting to database: {}", error.to_string());
            None
        }
    }
}

fn parse_timestamp(query: &str) -> Result<TimeRange, String> {
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

    Ok(TimeRange {
        before: before.map(|b| b.unwrap()),
        after: after.map(|a| a.unwrap()),
    })
}

fn parse_username(query: &str) -> Result<String, String> {
    let args = url::form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect::<HashMap<String, String>>();

    let username = args.get("username");
    match username {
        Some(user) => Ok(user.parse().unwrap()),
        None => Err(format!("Error parsing 'username'")),
    }
}

fn parse_form(form_chunk: Chunk) -> FutureResult<NewMessage, hyper::Error> {
    let mut form = url::form_urlencoded::parse(form_chunk.as_ref())
        .into_owned()
        .collect::<HashMap<String, String>>();

    if let Some(message) = form.remove("message") {
        let username = form.remove("username").unwrap_or(String::from("unknown"));
        futures::future::ok(NewMessage { username, message })
    } else {
        futures::future::err(hyper::Error::from(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Missing field 'message",
        )))
    }
}

fn write_to_db(
    new_message: NewMessage,
    db_connection: &PgConnection,
) -> FutureResult<i64, hyper::Error> {
    use schema::messages;

    let timestamp = diesel::insert_into(messages::table)
        .values(&new_message)
        .returning(messages::timestamp)
        .get_result(db_connection);

    match timestamp {
        Ok(timestamp) => futures::future::ok(timestamp),
        Err(err) => {
            error!("Error writing to database: {}", err.to_string());
            futures::future::err(hyper::Error::from(io::Error::new(
                io::ErrorKind::Other,
                "service error",
            )))
        }
    }
}

fn main() {
    env_logger::init();
    let address = "127.0.0.1:8000".parse().unwrap();
    let server = hyper::server::Http::new()
        .bind(&address, || Ok(Microservice {}))
        .unwrap();
    info!("Running microservice at {}", address);
    server.run().unwrap();
}
