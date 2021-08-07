use abraham::{listen_for_messages, spawn};

#[async_std::main]
async fn main() {
    for _ in 0..100 {
        spawn().await;
    }

    listen_for_messages().await;

    loop {}
}
