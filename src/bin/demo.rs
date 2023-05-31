use std::str::FromStr;

use aleo_rust::{Address, Testnet3};
use serde::Serialize;
use serde_json::from_str;

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum GameMessage {
    OpponentDisconnected {
        game_id: u64,
        pubkey: Address<Testnet3>,
    },
    OpponentConnected {
        game_id: u64,
        pubkey: Address<Testnet3>,
    },
    Role {
        // 连上ws后，server 通知角色分配
        game_id: u64,
        player1: Address<Testnet3>,
        player2: Address<Testnet3>,
    },
}

fn main() {
    let addr = Address::<Testnet3>::from_str(
        "aleo12m0ks7kd78ulf4669v2maynerc3jhj2ukkxyw6mdv6rag6xw8cpqdpm4vm",
    )
    .unwrap();
    let msg = GameMessage::Role {
        game_id: 1u64,
        player1: addr,
        player2: addr,
    };
    println!("{}", serde_json::to_string(&msg).unwrap());
}
