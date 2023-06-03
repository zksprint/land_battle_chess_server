use aleo_rust::{Address, Testnet3};
use axum::extract::ws::Message;
use serde::{Deserialize, Serialize};

use crate::game_logic::{Piece, PieceMove};

#[derive(Debug, Serialize, Deserialize, Clone)]
/*
json:
{"type":"role","game_id":1,"player1":"aleo12m0ks7kd78ulf4669v2maynerc3jhj2ukkxyw6mdv6rag6xw8cpqdpm4vm","player2":"aleo12m0ks7kd78ulf4669v2maynerc3jhj2ukkxyw6mdv6rag6xw8cpqdpm4vm"}
 */
#[serde(tag = "type", rename_all = "camelCase")]
#[allow(clippy::large_enum_variant)]
pub enum GameMessage {
    OpponentDisconnected {
        // 对手下线
        game_id: String,
    },
    Ready {
        game_id: String,
    },
    Hello {
        game_id: String,
    },
    Role {
        // 连上ws后，server 通知角色分配
        game_id: String,
        player1: Address<Testnet3>,
        player2: Address<Testnet3>,
    },
    Move {
        // 行棋方，通知server 行棋路线
        piece: Piece,
        x: u32, // 棋子坐标
        y: u32,
        target_x: u32, // 落子坐标
        target_y: u32,
        flag_x: Option<u32>,
        flag_y: Option<u32>,
    },
    PiecePos {
        // server 通知对手，行棋路线
        x: u32,
        y: u32,
        target_x: u32,
        target_y: u32,
    },
    Whisper {
        // 对手通知server，落子坐标棋子信息，如果piece 是司令，同时告知军棋坐标
        piece: Piece,
        x: u32,
        y: u32,
        flag_x: Option<u32>,
        flag_y: Option<u32>,
    },
    MoveResult(PieceMove),
}

impl TryInto<Message> for GameMessage {
    type Error = serde_json::Error;
    fn try_into(self) -> Result<Message, Self::Error> {
        Ok(Message::Text(serde_json::to_string(&self)?))
    }
}

#[derive(Debug, Deserialize)]
pub struct Join {
    pub access_code: String,
    pub pubkey: Address<Testnet3>,
}

#[derive(Serialize)]
pub enum AppResponse {
    Error(String),
    JoinResult { game_id: String },
}