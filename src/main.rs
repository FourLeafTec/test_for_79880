#![allow(unused_imports)]
use futures;
use log::*;
use pretty_env_logger;
use tokio::{io, spawn};

mod config;
mod udp;
mod ws;

#[tokio::main]
async fn main() {
    // init logging
    pretty_env_logger::init();
    info!("Init toll gate on board service.");
    let mut handles = vec![];
    handles.push(tokio::spawn(async {
        udp::start().await;
        ()
    }));
    handles.push(tokio::spawn(async {
        let _ = ws::start().await;
        ()
    }));

    futures::future::join_all(handles).await;
}
