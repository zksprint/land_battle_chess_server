use aleo_rust::{Address, Testnet3};
use axum::extract::ws::Message;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};

use crate::game_logic::{Piece, PieceMove};

#[serde_as]
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
        #[serde_as(as = "DisplayFromStr")]
        game_id: u64,
    },
    Ready {
        #[serde_as(as = "DisplayFromStr")]
        game_id: u64,
    },
    GameStart {
        #[serde_as(as = "DisplayFromStr")]
        game_id: u64,
        turn: Address<Testnet3>,
    },
    Hello {
        #[serde_as(as = "DisplayFromStr")]
        game_id: u64,
    },
    Role {
        // 连上ws后，server 通知角色分配
        #[serde_as(as = "DisplayFromStr")]
        game_id: u64,
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
#[serde_as]
pub enum AppResponse {
    Error(String),
    JoinResult {
        #[serde_as(as = "DisplayFromStr")]
        game_id: u64,
    },
}

#[derive(Debug, Deserialize)]
pub struct EnterGame {
    pub player: Address<Testnet3>,
    pub game_id: u64,
}
