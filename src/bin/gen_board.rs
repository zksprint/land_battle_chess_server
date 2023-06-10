use std::{fs::read_to_string, path::PathBuf};

use land_battle_chess::{board_utils::Board, game_logic::Piece};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "gen_board")]
struct Opt {
    #[structopt(long)]
    player2: bool,

    #[structopt(long)]
    path: PathBuf,
}

fn main() {
    let opt = Opt::from_args();
    let data = read_to_string(opt.path).unwrap();
    let pieces = serde_json::from_str::<Vec<Vec<String>>>(&data).unwrap();

    let pieces: Vec<Vec<_>> = pieces
        .into_iter()
        .map(|vec| vec.into_iter().map(Piece::from).collect::<Vec<Piece>>())
        .collect();

    let board = Board::gen(pieces, opt.player2);
    println!("{:?}", board);
    for i in 0..5 {
        println!("LINE{}={}", i, board.lines[i]);
    }

    let pieces = [
        243944965961916,
        243944752439345,
        243944969889843,
        243944969494674,
        243944969840532,
    ];
    let board = Board::new(pieces);
    println!("{:?}", board);
    println!("{:?}", board.get_piece(0, 6));
}
