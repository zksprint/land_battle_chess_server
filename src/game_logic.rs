use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

#[derive(Debug, PartialEq, PartialOrd, Eq, Deserialize_repr, Serialize_repr, Clone)]
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
    pub x: u32,
    pub y: u32,

    pub flag_x: u32,
    pub flag_y: u32,
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

    pub opp_victim: Piece,

    pub flag_x: u32,
    pub flag_y: u32,
}

pub const INVALID_X: u32 = 5u32;
pub const INVALID_Y: u32 = 12u32;

pub fn arb_piece(attacker: PieceInfo, target: PieceInfo) -> PieceMove {
    let attack_result: AttackResult;
    let mut opp_victim = Piece::Empty;
    let mut flag_x = INVALID_X;
    let mut flag_y = INVALID_Y;

    if target.piece == Piece::Empty {
        attack_result = AttackResult::SimpleMove;
    } else if attacker.piece == Piece::Bomb || target.piece == Piece::Bomb {
        //bomb
        attack_result = AttackResult::Draw;
        opp_victim = target.piece;
    } else if target.piece == Piece::Landmine {
        //landmine
        if attacker.piece == Piece::Engineer {
            //engineer
            attack_result = AttackResult::Win;
            opp_victim = target.piece;
        } else {
            attack_result = AttackResult::Lose;
        }
    } else if attacker.piece > target.piece {
        attack_result = AttackResult::Win;
        opp_victim = target.piece;
    } else if attacker.piece == target.piece {
        attack_result = AttackResult::Draw;
        opp_victim = target.piece;
    } else {
        attack_result = AttackResult::Lose;
    }

    if attacker.piece == Piece::FieldMarshal
        && (attack_result == AttackResult::Draw || attack_result == AttackResult::Lose)
    {
        flag_x = attacker.flag_x;
        flag_y = attacker.flag_y;
    } else if opp_victim == Piece::FieldMarshal
        && (attack_result == AttackResult::Draw || attack_result == AttackResult::Win)
    {
        flag_x = target.flag_x;
        flag_y = target.flag_y;
    }

    PieceMove {
        x: attacker.x,
        y: attacker.y,
        target_x: target.x,
        target_y: target.y,
        attack_result,
        opp_victim,
        flag_x,
        flag_y,
    }
}
