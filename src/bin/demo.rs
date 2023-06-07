use std::str::FromStr;

use aleo_rust::{Address, Testnet3};
use land_battle_chess_rs::{
    game_logic::{AttackResult, PieceMove},
    types::GameMessage,
};

fn main() {
    let _addr = Address::<Testnet3>::from_str(
        "aleo12m0ks7kd78ulf4669v2maynerc3jhj2ukkxyw6mdv6rag6xw8cpqdpm4vm",
    )
    .unwrap();
    let piece_move = PieceMove {
        x: 1,
        y: 1,
        target_x: 1,
        target_y: 2,
        attack_result: AttackResult::Draw,
        opp_flag_x: None,
        opp_flag_y: None,
        flag_x: Some(0),
        flag_y: Some(0),
        game_winner: 0,
    };
    let msg = GameMessage::MoveResult(piece_move);
    let json_text = serde_json::to_string(&msg).unwrap();
    let msg: GameMessage = serde_json::from_str(&json_text).unwrap();
    println!("msg:{:?}", msg);
    let msg = GameMessage::OpponentDisconnected { game_id: 123 };
    let json_text = serde_json::to_string(&msg).unwrap();
    println!("text:{}", json_text);
    let msg: GameMessage = serde_json::from_str(&json_text).unwrap();
    println!("msg:{:?}", msg);
}
