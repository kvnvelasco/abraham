use lazy_static::lazy_static;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use async_std::net::{SocketAddr, TcpListener, TcpStream};
use async_tungstenite::tungstenite::protocol::Message;
use futures::pin_mut;
use futures::prelude::*;
use futures::TryStreamExt;

lazy_static! {
    static ref CONNECTION_MAP: Arc<RwLock<HashMap<Uuid, futures::channel::mpsc::UnboundedSender<Message>>>> =
        Default::default();
    static ref UPSTREAM_CONNECTION: Arc<RwLock<Option<futures::channel::mpsc::UnboundedSender<Message>>>> =
        Default::default();
}

pub async fn listen_for_connections(port: usize) -> TcpListener {
    let listener = async_std::net::TcpListener::bind(format!("127.0.0.1:{}", port))
        .await
        .expect("Could not listen");
    listener
}

pub async fn send_to_downstream(message: Message) {
    let id = message.to_string();
    if id.is_empty() {
        return;
    }

    let (id, message) = id.split_once(":").unwrap();

    println!("Upstream sending {} to {} downstream", message, &id);
    let id = Uuid::parse_str(&id).expect("Invalid uuid!");
    let mut lock = CONNECTION_MAP.write();
    let tx = lock.get_mut(&id).unwrap();
    tx.unbounded_send(Message::Text(message.to_string()));
}

pub async fn handle_upstream_connection(stream: TcpStream, addr: SocketAddr) {
    let socket = async_tungstenite::accept_async(stream)
        .await
        .expect("Unable to start socket");

    use futures::stream::{Stream, StreamExt};
    let (tx, rx) = futures::channel::mpsc::unbounded();
    let (sink, mut source) = socket.split();

    let process_source = async {
        while let Some(Ok(message)) = source.next().await {
            send_to_downstream(message).await;
        }
    };

    {
        let mut lock = UPSTREAM_CONNECTION.write();
        *lock = Some(tx);
    }

    let process_upstream = rx.map(Ok).forward(sink);
    pin_mut!(process_source, process_upstream);

    futures::future::select(process_source, process_upstream).await;
    println!("Upstream disconnected!, shutting down");

    std::process::exit(2);
}

pub async fn handle_connection(stream: TcpStream, addr: SocketAddr) {
    let id = Uuid::new_v4();
    let socket = async_tungstenite::accept_async(stream)
        .await
        .expect("Unable to start socket");

    println!("Started connection with id {}", &id);

    {
        let mut lock = UPSTREAM_CONNECTION.read();
        if let Some(upstream) = &*lock {
            upstream.unbounded_send(Message::Text(format!("new_session:{}", id.to_string())));
        } else {
            println!("Cannot even. No upstream yet.");
            return;
        }
    }

    use futures::stream::{Stream, StreamExt};

    let (sink, source) = socket.split();
    let (tx, rx) = futures::channel::mpsc::unbounded();

    CONNECTION_MAP.write().insert(id.clone(), tx);

    let recv = source.try_for_each(|message| {
        let lock = UPSTREAM_CONNECTION.read();
        if let Some(upstream) = &*lock {
            upstream.unbounded_send(message);
        } else {
            panic!("Got socket connection without upstream!")
        }

        futures::future::ok(())
    });

    let send = rx.map(Ok).forward(sink);

    pin_mut!(send, recv);

    futures::future::select(send, recv).await;

    println!("{}:{} disconnected", &id, &addr);
    CONNECTION_MAP.write().remove(&id);
}
