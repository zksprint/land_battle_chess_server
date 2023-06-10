# How to Build and Run
1、gen_board

We use 4-bit to represent the pieces in the game, manually constructing the chessboard is complex, please use the gen_board tool to assist in building the chessboard.

**build**
```
cargo build --release --bin gen_board
```

**.json config**

Refer to the [configuration file](https://github.com/zksprint/land_battle_chess_server/tree/main/data).

**run**
```
./target/release/gen_board --path ./data/player1.json
```
```
./target/release/gen_board --path ./data/player2.json --player2
```

2、land_battle_chess server

The game UI is under development, and the server is not yet able to run independently.