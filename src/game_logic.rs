use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use strum::FromRepr;

#[derive(
    Debug, PartialEq, PartialOrd, Eq, Deserialize_repr, Serialize_repr, Copy, Clone, FromRepr,
)]
#[repr(u64)]
pub enum Piece {
    Empty = 0,
    Flag = 1,
    Bomb = 2,
    Landmine = 3,
    Engineer = 4,
    Lieutenant = 5,
    Captain = 6,
    Major = 7,
    Colonel = 8,
    Brigadier = 9,
    MajorGeneral = 10,
    General = 11,
    FieldMarshal = 12,
    Unchanged = 15,
    Opponent = 16,
}

pub struct PieceInfo {
    pub piece: Piece,

    pub flag_x: Option<u32>,
    pub flag_y: Option<u32>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct MovePos {
    pub x: u32,
    pub y: u32,
    pub target_x: u32,
    pub target_y: u32,
}

#[derive(Debug, PartialEq, PartialOrd, Eq, Deserialize_repr, Serialize_repr, Clone)]
#[repr(u32)]
pub enum AttackResult {
    SimpleMove = 0,
    Win = 1,
    Draw = 2,
    Lose = 3,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct PieceMove {
    pub x: u32,
    pub y: u32,
    pub target_x: u32,
    pub target_y: u32,

    pub attack_result: AttackResult,

    pub flag_x: Option<u32>,
    pub flag_y: Option<u32>,
    pub opp_flag_x: Option<u32>,
    pub opp_flag_y: Option<u32>,

    pub game_winner: u32,
}

pub fn compare_piece(attacker: PieceInfo, target: PieceInfo, move_pos: MovePos) -> PieceMove {
    let attack_result: AttackResult;
    let mut victim = Piece::Empty;
    let mut opp_victim = Piece::Empty;
    let mut flag_x = None;
    let mut flag_y = None;
    let mut opp_flag_x = None;
    let mut opp_flag_y = None;
    let mut game_winner = 0;

    if target.piece == Piece::Empty {
        attack_result = AttackResult::SimpleMove;
    } else if attacker.piece == Piece::Bomb || target.piece == Piece::Bomb {
        //bomb
        attack_result = AttackResult::Draw;
    } else if target.piece == Piece::Landmine {
        //landmine
        if attacker.piece == Piece::Engineer {
            //engineer
            attack_result = AttackResult::Win;
        } else {
            attack_result = AttackResult::Lose;
        }
    } else if attacker.piece > target.piece {
        attack_result = AttackResult::Win;
    } else if attacker.piece == target.piece {
        attack_result = AttackResult::Draw;
    } else {
        attack_result = AttackResult::Lose;
    }

    match attack_result {
        AttackResult::Win => {
            opp_victim = target.piece;
        }
        AttackResult::Draw => {
            opp_victim = target.piece;
            victim = attacker.piece;
        }
        AttackResult::Lose => {
            victim = attacker.piece;
        }
        _ => {}
    }

    if victim == Piece::FieldMarshal {
        flag_x = attacker.flag_x;
        flag_y = attacker.flag_y;
    }

    if opp_victim == Piece::FieldMarshal {
        opp_flag_x = target.flag_x;
        opp_flag_y = target.flag_y;
    }

    if victim == Piece::Flag {
        game_winner = 2;
    } else if opp_victim == Piece::Flag {
        game_winner = 1;
    }

    PieceMove {
        x: move_pos.x,
        y: move_pos.y,
        target_x: move_pos.target_x,
        target_y: move_pos.target_y,
        attack_result,
        flag_x,
        flag_y,
        opp_flag_x,
        opp_flag_y,
        game_winner,
    }
}
