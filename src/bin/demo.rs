use std::str::FromStr;

use aleo_rust::{Address, Testnet3};
use land_battle_chess_rs::{
    game_logic::{AttackResult, Piece, PieceMove, INVALID_X, INVALID_Y},
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
        opp_victim: Piece::Bomb,
        flag_x: INVALID_X,
        flag_y: INVALID_Y,
    };
    let msg = GameMessage::MoveResult(piece_move);
    let json_text = serde_json::to_string(&msg).unwrap();
    let msg: GameMessage = serde_json::from_str(&json_text).unwrap();
    println!("msg:{:?}", msg);
}
