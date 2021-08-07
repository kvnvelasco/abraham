use async_std::channel::unbounded;
use async_tungstenite::tungstenite::Message;
use futures::pin_mut;
use futures::prelude::*;
use futures::StreamExt;
use lazy_static::lazy_static;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::process::{Child, Stdio};
use std::sync::Arc;
use uuid::Uuid;

lazy_static! {
    static ref CHILDREN: Arc<RwLock<HashMap<Uuid, futures::channel::mpsc::UnboundedSender<Message>>>> =
        Default::default();
}

pub async fn listen_for_messages() {
    println!("To send a message to a child: type a message with the format");
    println!("{{uuid}}:{{message}}");
    loop {
        let mut buffer = String::new();
        std::io::stdin().read_line(&mut buffer);

        if let Some('\n') = buffer.chars().next_back() {
            buffer.pop();
        }
        if let Some('\r') = buffer.chars().next_back() {
            buffer.pop();
        }

        let (uuid, message) = buffer.trim().split_once(":").unwrap();
        println!("Sending {} to child {}", &message, &uuid);

        let uuid = Uuid::parse_str(uuid).unwrap();
        let message = Message::Text(format!("{}:{}", uuid.to_string(), message));
        send_to_child(uuid, message).await;
    }
}
pub async fn send_to_child(uuid: Uuid, message: Message) {
    let lock = CHILDREN.read();
    let tx = lock.get(&uuid).unwrap();
    tx.unbounded_send(message);
}

pub async fn spawn() {
    let port = portpicker::pick_unused_port().expect("No free ports left");
    let target = std::env::current_dir()
        .unwrap()
        .join("target/debug/socket-server");
    println!("=================================");
    std::process::Command::new(&target)
        .arg("--upstream-port")
        .arg(&port.to_string())
        .spawn()
        .unwrap();
    async_std::task::sleep(std::time::Duration::from_millis(10)).await;

    async_std::task::spawn(async move {
        let stream = async_std::net::TcpStream::connect(format!("127.0.0.1:{}", &port))
            .await
            .expect("Can't connect to child");

        let (socket, _response) =
            async_tungstenite::client_async(format!("ws://127.0.0.1:{}", &port), stream)
                .await
                .unwrap();
        let (tx, rx) = futures::channel::mpsc::unbounded();
        let (sink, source) = socket.split();

        let recv = source.try_for_each(move |message| {
            let message = message.to_string();
            if message.contains("new_session") {
                println!("Child has new session: {}", message.to_string());
                let (_, id) = message.split_once(":").unwrap();
                let uuid = Uuid::parse_str(&id).unwrap();
                CHILDREN.write().insert(uuid, tx.clone());
            } else {
                println!("A child has said {}", message.to_string());
            }
            futures::future::ok(())
        });

        let send = rx.map(Ok).forward(sink);

        pin_mut!(send, recv);

        futures::future::select(send, recv).await;
    });
}
