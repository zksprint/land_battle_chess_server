use crate::game_logic::Piece;
use tabled::{Table, Tabled};

#[derive(Default)]
pub struct Board {
    pub lines: [u64; 5],
}

impl From<String> for Piece {
    fn from(v: String) -> Piece {
        match v.as_str() {
            "军棋" => Piece::Flag,
            "炸弹" => Piece::Bomb,
            "地雷" => Piece::Landmine,
            "工兵" => Piece::Engineer,
            "排长" => Piece::Lieutenant,
            "连长" => Piece::Captain,
            "营长" => Piece::Major,
            "团长" => Piece::Colonel,
            "旅长" => Piece::Brigadier,
            "师长" => Piece::MajorGeneral,
            "军长" => Piece::General,
            "司令" => Piece::FieldMarshal,
            "对手" => Piece::Opponent,
            _ => Piece::Empty,
        }
    }
}

impl Board {
    pub fn new(lines: [u64; 5]) -> Self {
        Board { lines }
    }

    pub fn gen(pieces: Vec<Vec<Piece>>, is_player2: bool) -> Self {
        assert_eq!(pieces.len(), 6);
        let mut board = Board::default();
        for y in 0..6u64 {
            for x in 0..5u64 {
                let piece = pieces[y as usize][x as usize];
                let y = if is_player2 { 11 - y } else { y };
                board.place_piece(x, y, piece);
            }
        }

        for y in 6..12u64 {
            for x in 0..5u64 {
                if y == 3 && (x == 1 || x == 3) {
                    continue;
                }

                if y == 5 && (x == 1 || x == 3) {
                    continue;
                }

                let y = if is_player2 { 11 - y } else { y };
                board.place_piece(x, y, Piece::Opponent);
            }
        }

        board
    }

    pub fn place_piece(&mut self, x: u64, y: u64, piece: Piece) -> bool {
        let square = self.get_piece(x, y);
        if square != Piece::Empty {
            return false;
        }

        let row = y * 4;
        let piece = (piece as u64) << row;
        self.lines[x as usize] |= piece;
        true
    }

    pub fn get_piece(&self, x: u64, y: u64) -> Piece {
        let line = self.lines[x as usize];
        Piece::from_repr(Self::get_piece_from_line(line, y)).unwrap()
    }

    fn get_piece_from_line(x: u64, y: u64) -> u64 {
        let mask = 0xf;
        let row = y * 4;
        ((mask << row) & x) >> row
    }
}

pub fn piece_name(piece: Piece) -> &'static str {
    match piece {
        Piece::Flag => "军棋",
        Piece::Bomb => "炸弹",
        Piece::Landmine => "地雷",
        Piece::Engineer => "工兵",
        Piece::Lieutenant => "排长",
        Piece::Captain => "连长",
        Piece::Major => "营长",
        Piece::Colonel => "团长",
        Piece::Brigadier => "旅长",
        Piece::MajorGeneral => "师长",
        Piece::General => "军长",
        Piece::FieldMarshal => "司令",
        Piece::Opponent => "XXX",
        _ => "",
    }
}

impl std::fmt::Debug for Board {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[derive(Tabled, Default)]
        struct BoardRow {
            pub l0: String,
            pub l1: String,
            pub l2: String,
            pub l3: String,
            pub l4: String,
        }

        let mut rows: Vec<_> = (0..12).map(|_| BoardRow::default()).collect();
        for row in 0..12 {
            let piece = self.get_piece(0, row);
            rows[row as usize].l0 = piece_name(piece).into();
        }
        for row in 0..12 {
            let piece = self.get_piece(1, row);
            rows[row as usize].l1 = piece_name(piece).into();
        }
        for row in 0..12 {
            let piece = self.get_piece(2, row);
            rows[row as usize].l2 = piece_name(piece).into();
        }
        for row in 0..12 {
            let piece = self.get_piece(3, row);
            rows[row as usize].l3 = piece_name(piece).into();
        }
        for row in 0..12 {
            let piece = self.get_piece(4, row);
            rows[row as usize].l4 = piece_name(piece).into();
        }

        write!(f, "{}", Table::new(rows))
    }
}
