use socket_server::{handle_connection, handle_upstream_connection, listen_for_connections};
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
struct Opt {
    #[structopt(short, long)]
    upstream_port: usize,
}

#[async_std::main]
async fn main() {
    let opt = Opt::from_args();

    let server = async_std::task::spawn(async move {
        let port = portpicker::pick_unused_port().expect("No free ports left");
        let listener = listen_for_connections(port as usize).await;
        println!("server listening on: {}", listener.local_addr().unwrap());
        while let (stream, address) = listener.accept().await.expect("the fuck") {
            async_std::task::spawn(handle_connection(stream, address));
        }
    });

    let upstream = async_std::task::spawn(async move {
        let listener = listen_for_connections(opt.upstream_port).await;
        println!("Upstream listening on: {}", listener.local_addr().unwrap());
        while let (stream, address) = listener.accept().await.expect("the fuck") {
            async_std::task::spawn(handle_upstream_connection(stream, address));
        }
    });

    futures::pin_mut!(server, upstream);

    futures::future::select(server, upstream).await;
}
