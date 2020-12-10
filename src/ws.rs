#![allow(unused_imports)]
use futures_util::{SinkExt, StreamExt};
use log::*;
use pretty_env_logger;
use std::{net::SocketAddr, time::Duration};
use tokio::io;
use tokio::net::{TcpListener, TcpStream};
use tokio::spawn;
use tokio_tungstenite::{
    accept_async,
    tungstenite::{Error, Message, Result},
};

pub async fn start() -> io::Result<()> {
    info!("Init ws.");
    let addr = "0.0.0.0:9000";
    let listener = TcpListener::bind(&addr).await?;
    info!("WS Listening on: {}", addr);

    while let Ok((stream, _)) = listener.accept().await {
        let mut websocket = accept_async(stream).await.expect("Failed to accept.");
        while let Some(msg) = websocket.next().await {
            let msg = msg.expect("msg err");
            if msg.is_text() || msg.is_binary() {
                debug!("receive msg:{}", &msg);
                websocket.send(msg).await.expect("send msg err");
            }
        }
    }
    Ok(())
}
