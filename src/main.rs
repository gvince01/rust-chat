extern crate hyper;
extern crate futures;

#[macro_use]
extern crate log;
extern crate env_logger;

use hyper::server::{Request, Response, Service};

use futures::future::Future;

struct Mircoservice;

impl Service for Mircoservice {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = Box<Future<Item = Self::Response, Error = Self::Error>>;

    fn call(&self, request: Request) -> Self::Future {
        info!("Microservice recieved a request: {:?}", request);
        Box::new(futures::future::Ok(Response::new()))
    }

}

fn main() {
    println!("Hello, world!");
}
