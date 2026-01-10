use anyhow::Result;
use argh::FromArgs;
use btclib::types::Blockchain;
use dashmap::DashMap;
use static_init::dynamic;
use std::path::Path;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;

mod handler;
mod util;

#[dynamic]
pub static BLOCKCHAIN: RwLock<Blockchain> = RwLock::new(Blockchain::new());

#[dynamic]
pub static NODES: DashMap<String, TcpStream> = DashMap::new();

#[derive(FromArgs)]
/// toy blockchain node
struct Args {
    #[argh(option, default = "9000")]
    /// port number
    port: u16,

    #[argh(option, default = "String::from(\"./blockchain.cbor\")")]
    /// blockchain file
    blockchain_file: String,

    #[argh(positional)]
    /// address of nodes
    nodes: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Args = argh::from_env();

    let port = args.port;
    let blockchain_file = args.blockchain_file;
    let nodes = args.nodes;

    if Path::new(&blockchain_file).exists() {
        util::load_blockchain(&blockchain_file).await?;
    } else {
        println!("blockchain file does not exist!");

        // 주어진 nodes 주소를 순차적으로 connection 맺는다 
        util::populate_connections(&nodes).await?;
        println!("total amount of known nodes: {}", NODES.len());

        if nodes.is_empty() {
            println!("no initial nodes provided, starting as a seed node");
        } else {
            let (longest_name, longest_count) = util::find_longest_chain_node().await?;

            // request the blockchain from the node with the longest blockchain
            util::download_blockchain(&longest_name, longest_count).await?;

            println!("blockchain downloaded from {}", longest_name);

            // utxo를 채워 넣는다 
            {
                let mut blockchain = BLOCKCHAIN.write().await;
                blockchain.rebuild_utxos();
            }

            // 난이도 조정 
            {
                let mut blockchain = BLOCKCHAIN.write().await;
                blockchain.try_adjust_target();
            }
        }

        let addr = format!("0.0.0.0:{}", port);
        let listener = TcpListener::bind(&addr).await?;
        println!("Listening on {}", addr);

        // 주기적으로 mempool 내 오래 잔존한 tx를 제거함 
        tokio::spawn(util::cleanup());

        // 주기적으로 blockchain 스냅샷 떠서 저장함  
        tokio::spawn(util::save(blockchain_file.clone()));

        loop {
            let (socket, _) = listener.accept().await?;

            // message에 따른 핸들러들  
            tokio::spawn(handler::handle_connection(socket));
        }
    }

    Ok(())
}
