/**
 * MIT License
 *
 * Copyright (c) 2017 NeoSmart Technologies <https://neosmart.net/>
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 * SOFTWARE.
 */
use log::*;
use rand;
use std::collections::HashMap;
use std::env;
use std::net::{Ipv4Addr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const TIMEOUT: u64 = 3 * 60 * 100; //3 minutes

pub async fn start() {
    info!("Init udp.");

    let local_port: i32 = 9901;
    let remote_port: i32 = 9901;
    let remote_host = "192.168.64.1";
    let bind_addr = "0.0.0.0";
    forward(&bind_addr, local_port, &remote_host, remote_port);
}

fn forward(bind_addr: &str, local_port: i32, remote_host: &str, remote_port: i32) {
    let local_addr = format!("{}:{}", bind_addr, local_port);
    let local = UdpSocket::bind(&local_addr).expect(&format!("Unable to bind to {}", &local_addr));
    // local.set_broadcast(true).expect("set broadcast fail");
    // local
    //     .join_multicast_v4(&Ipv4Addr::new(224, 0, 0, 1), &Ipv4Addr::new(0, 0, 0, 0))
    //     .expect("join multicast fail");
    debug!("UDP Listening on {}", local.local_addr().unwrap());

    let remote_addr = format!("{}:{}", remote_host, remote_port);

    let responder = local.try_clone().expect(&format!(
        "Failed to clone primary listening address socket {}",
        local.local_addr().unwrap()
    ));
    let (main_sender, main_receiver) = channel::<(_, Vec<u8>)>();
    thread::spawn(move || {
        debug!("Started new thread to deal out responses to clients");
        loop {
            let (dest, buf) = main_receiver.recv().unwrap();
            let to_send = buf.as_slice();
            responder.send_to(to_send, dest).expect(&format!(
                "Failed to forward response from upstream server to client {}",
                dest
            ));
        }
    });

    let mut client_map = HashMap::new();
    let mut buf = [0; 64 * 1024];
    loop {
        let (num_bytes, src_addr) = local.recv_from(&mut buf).expect("Didn't receive data");

        //we create a new thread for each unique client
        let mut remove_existing = false;
        loop {
            debug!("Received packet from client {}", src_addr);

            let mut ignore_failure = true;
            let client_id = format!("{}", src_addr);

            if remove_existing {
                debug!("Removing existing forwarder from map.");
                client_map.remove(&client_id);
            }

            let sender = client_map.entry(client_id.clone()).or_insert_with(|| {
                //we are creating a new listener now, so a failure to send shoud be treated as an error
                ignore_failure = false;

                let local_send_queue = main_sender.clone();
                let (sender, receiver) = channel::<Vec<u8>>();
                let remote_addr_copy = remote_addr.clone();
                thread::spawn(move|| {
                    //regardless of which port we are listening to, we don't know which interface or IP
                    //address the remote server is reachable via, so we bind the outgoing
                    //connection to 0.0.0.0 in all cases.
                    let temp_outgoing_addr = format!("0.0.0.0:{}", 1024 + rand::random::<u16>());
                    debug!("Establishing new forwarder for client {} on {}", src_addr, &temp_outgoing_addr);
                    let upstream_send = UdpSocket::bind(&temp_outgoing_addr)
                        .expect(&format!("Failed to bind to transient address {}", &temp_outgoing_addr));
                    let upstream_recv = upstream_send.try_clone()
                        .expect("Failed to clone client-specific connection to upstream!");

                    let mut timeouts : u64 = 0;
                    let timed_out = Arc::new(AtomicBool::new(false));

                    let local_timed_out = timed_out.clone();
                    thread::spawn(move|| {
                        let mut from_upstream = [0; 64 * 1024];
                        upstream_recv.set_read_timeout(Some(Duration::from_millis(TIMEOUT + 100))).unwrap();
                        loop {
                            match upstream_recv.recv_from(&mut from_upstream) {
                                Ok((bytes_rcvd, _)) => {
                                    let to_send = from_upstream[..bytes_rcvd].to_vec();
                                    local_send_queue.send((src_addr, to_send))
                                        .expect("Failed to queue response from upstream server for forwarding!");
                                },
                                Err(_) => {
                                    if local_timed_out.load(Ordering::Relaxed) {
                                        debug!("Terminating forwarder thread for client {} due to timeout", src_addr);
                                        break;
                                    }
                                }
                            };
                        }
                    });

                    loop {
                        match receiver.recv_timeout(Duration::from_millis(TIMEOUT)) {
                            Ok(from_client) =>  {
                                upstream_send.send_to(from_client.as_slice(), &remote_addr_copy)
                                    .expect(&format!("Failed to forward packet from client {} to upstream server!", src_addr));
                                timeouts = 0; //reset timeout count
                            },
                            Err(_) => {
                                timeouts += 1;
                                if timeouts >= 10 {
                                    debug!("Disconnecting forwarder for client {} due to timeout", src_addr);
                                    timed_out.store(true, Ordering::Relaxed);
                                    break;
                                }
                            }
                        };
                    }
                });
                sender
            });

            let to_send = buf[..num_bytes].to_vec();
            match sender.send(to_send) {
                Ok(_) => {
                    break;
                }
                Err(_) => {
                    if !ignore_failure {
                        panic!(
                            "Failed to send message to datagram forwarder for client {}",
                            client_id
                        );
                    }
                    //client previously timed out
                    debug!(
                        "New connection received from previously timed-out client {}",
                        client_id
                    );
                    remove_existing = true;
                    continue;
                }
            }
        }
    }
}
