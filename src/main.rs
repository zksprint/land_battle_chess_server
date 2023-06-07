use std::convert::TryInto;
use std::{collections::HashMap, net::SocketAddr, path::PathBuf, str::FromStr, sync::Arc};

use aleo_rust::{Address, PrivateKey, Testnet3};
use axum::{
    body::{self},
    extract::{
        ws::{Message, WebSocket},
        Path, Query, State, WebSocketUpgrade,
    },
    http::{HeaderValue, Method, Response, StatusCode},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use colored::Colorize;
use eyre::{bail, eyre, Context};
use indoc::indoc;

use futures::stream::SplitSink;
use futures::{sink::SinkExt, stream::StreamExt};
use land_battle_chess_rs::game_logic::{compare_piece, MovePos, PieceInfo};
use land_battle_chess_rs::{setup_log_dispatch, types::*};
use log::{error, info, warn};
use structopt::StructOpt;

use tokio::sync::{
    mpsc::{channel, unbounded_channel, Sender, UnboundedReceiver, UnboundedSender},
    RwLock,
};

use tower_http::cors::CorsLayer;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};

#[derive(Debug, StructOpt)]
#[structopt(name = "land_battle")]
struct Opt {
    #[structopt(long)]
    log_path: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let opt = Opt::from_args();
    setup_log_dispatch(opt.log_path)?
        .level(log::LevelFilter::Error)
        .level_for("land_battle_chess_rs", log::LevelFilter::Trace)
        .apply()?;

    banner();
    dotenv::dotenv()?;

    let priv_key = std::env::var("ARBITER_PRIV_KEY").wrap_err("no arbiter privkey")?;
    let arbiter = PrivateKey::<Testnet3>::from_str(&priv_key)
        .map_err(|e| eyre!(e))
        .wrap_err("parse arbiter privkey")?;

    let app_state = App::init(arbiter);
    let app = Router::new()
        .route("/join", get(join))
        .route("/join/:pubkey", get(join_get))
        .route("/game", get(enter_game))
        .layer(
            CorsLayer::new()
                .allow_origin("http://localhost:8080".parse::<HeaderValue>().unwrap())
                .allow_methods([Method::GET, Method::POST]),
        )
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::default().include_headers(true)),
        )
        .with_state(app_state);

    let addr = SocketAddr::from_str("127.0.0.1:3000").unwrap();
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
    Ok(())
}

type GameId = u64;

struct App {
    user_map: HashMap<Address<Testnet3>, User>,
    game_map: HashMap<GameId, Game>,
    arbiter: (PrivateKey<Testnet3>, Address<Testnet3>),
}

impl App {
    fn init(arbiter: PrivateKey<Testnet3>) -> Arc<RwLock<App>> {
        let pubkey = Address::try_from(arbiter).unwrap();
        let app = App {
            arbiter: (arbiter, pubkey),
            user_map: HashMap::new(),
            game_map: HashMap::new(),
        };
        Arc::new(RwLock::new(app))
    }
}

type AppState = Arc<RwLock<App>>;

#[derive(Clone)]
struct User {
    pubkey: Address<Testnet3>,
    access_code: String,
    game_id: Option<GameId>,
}

#[derive(Debug, PartialEq, Eq)]
enum PlayerState {
    Disconnected,
    Connected,
    Ready,
}

struct Player {
    pubkey: Address<Testnet3>,
    state: PlayerState,
    piece: Option<PieceInfo>,
    move_pos: Option<MovePos>,
}

#[derive(Debug)]
struct PlayerConn {
    pubkey: Address<Testnet3>,
    ws_tx: SplitSink<WebSocket, Message>,
    exit_signal: Sender<()>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
enum GameServiceMsg {
    PlayerConnected(PlayerConn),
    GameMessage(Address<Testnet3>, GameMessage),
}

type GameServiceSender = UnboundedSender<GameServiceMsg>;

struct GameService {
    game_id: GameId,
    arbiter: Address<Testnet3>,
    players: (Player, Player),
    cur_player: Address<Testnet3>,
}

#[derive(Debug)]
struct Game {
    players: (Address<Testnet3>, Address<Testnet3>),
    tx: GameServiceSender,
}

impl GameService {
    async fn run(mut self, mut rx: UnboundedReceiver<GameServiceMsg>, _app_state: AppState) {
        let (game_id, player1, player2, arbiter) = (
            self.game_id,
            self.players.0.pubkey,
            self.players.1.pubkey,
            self.arbiter,
        );
        let mut conns = (None, None);
        while let Some(data) = rx.recv().await {
            match data {
                GameServiceMsg::PlayerConnected(mut conn) => match self.player_mut(conn.pubkey) {
                    Some(player) => {
                        if player.state != PlayerState::Disconnected {
                            todo!()
                        } else {
                            if let Err(e) = conn
                                .ws_tx
                                .send(
                                    GameMessage::Role {
                                        game_id,
                                        arbiter,
                                        player1,
                                        player2,
                                    }
                                    .try_into()
                                    .unwrap(),
                                )
                                .await
                            {
                                warn!("[{}] send role to {}, error: {:?}", game_id, conn.pubkey, e);
                                continue;
                            }
                            player.state = PlayerState::Connected;
                            if conn.pubkey == player1 {
                                conns.0 = Some(conn)
                            } else {
                                conns.1 = Some(conn)
                            };
                        }
                    }
                    None => {
                        conn.exit_signal.send(()).await.unwrap();
                    }
                },

                GameServiceMsg::GameMessage(pubkey, msg) => {
                    if let (Some(player1), Some(player2)) = (&mut conns.0, &mut conns.1) {
                        let (tx, opp_tx) = if pubkey == player1.pubkey {
                            (&mut player1.ws_tx, &mut player2.ws_tx)
                        } else {
                            (&mut player2.ws_tx, &mut player1.ws_tx)
                        };
                        if let Err(e) = self.process_player_message(msg, pubkey, tx, opp_tx).await {
                            error!("process player:{} message, error:{:?}", pubkey, e);
                        }
                    }
                }
            }
        }
    }

    async fn process_player_message(
        &mut self,
        msg: GameMessage,
        pubkey: Address<Testnet3>,
        player_tx: &mut SplitSink<WebSocket, Message>,
        opp_tx: &mut SplitSink<WebSocket, Message>,
    ) -> eyre::Result<()> {
        let game_id = self.game_id;
        match msg {
            GameMessage::Ready { .. } => {
                let player = self.player_mut(pubkey).unwrap();
                player.state = PlayerState::Ready;

                if let Some(opp) = self.opponent(pubkey) {
                    if opp.state == PlayerState::Ready {
                        let msg: Message = GameMessage::GameStart {
                            game_id,
                            turn: self.cur_player,
                        }
                        .try_into()
                        .unwrap();
                        _ = player_tx.send(msg.clone()).await;
                        _ = opp_tx.send(msg).await;
                    }
                }
            }
            GameMessage::Move {
                piece,
                x,
                y,
                target_x,
                target_y,
                flag_x,
                flag_y,
            } => {
                if self.cur_player != pubkey {
                    warn!("[{}] not {} turn", game_id, pubkey);
                    return Ok(());
                };

                let player = self.player_mut(pubkey).unwrap();
                if player.piece.is_some() {
                    warn!("[{}] player:{} has piece", game_id, player.pubkey);
                    return Ok(());
                }

                player.piece = Some(PieceInfo {
                    piece,
                    flag_x,
                    flag_y,
                });
                let move_pos = MovePos {
                    x,
                    y,
                    target_x,
                    target_y,
                };
                player.move_pos = Some(move_pos.clone());

                let msg: Message = GameMessage::PiecePos(move_pos).try_into().unwrap();
                opp_tx.send(msg).await.wrap_err("send opp")?;
            }
            GameMessage::Whisper {
                piece,
                flag_x,
                flag_y,
                ..
            } => {
                if self.cur_player == pubkey {
                    warn!("[{}] unexpect whisper from {}", game_id, pubkey);
                    return Ok(());
                };

                let target = PieceInfo {
                    piece,
                    flag_x,
                    flag_y,
                };
                let player = self.opponent_mut(pubkey).unwrap();
                let (attacker, move_pos) = (
                    player.piece.take().unwrap(),
                    player.move_pos.take().unwrap(),
                );
                let piece_move = compare_piece(attacker, target, move_pos);

                self.cur_player = pubkey;
                let msg: Message = GameMessage::MoveResult(piece_move).try_into().unwrap();
                _ = player_tx.send(msg.clone()).await;
                _ = opp_tx.send(msg).await;
            }
            _ => {}
        }
        Ok(())
    }

    fn opponent(&self, player: Address<Testnet3>) -> Option<&Player> {
        if self.players.0.pubkey == player {
            Some(&self.players.1)
        } else if self.players.1.pubkey == player {
            Some(&self.players.0)
        } else {
            None
        }
    }

    fn opponent_mut(&mut self, player: Address<Testnet3>) -> Option<&mut Player> {
        if self.players.0.pubkey == player {
            Some(&mut self.players.1)
        } else if self.players.1.pubkey == player {
            Some(&mut self.players.0)
        } else {
            None
        }
    }

    /*
       fn player(&self, player: Address<Testnet3>) -> Option<&Player> {
           if self.players.0.pubkey == player {
               Some(&self.players.0)
           } else if self.players.1.pubkey == player {
               Some(&self.players.1)
           } else {
               None
           }
       }
    */
    fn player_mut(&mut self, player: Address<Testnet3>) -> Option<&mut Player> {
        if self.players.0.pubkey == player {
            Some(&mut self.players.0)
        } else if self.players.1.pubkey == player {
            Some(&mut self.players.1)
        } else {
            None
        }
    }

    fn new(
        game_id: GameId,
        arbiter: Address<Testnet3>,
        player1: Address<Testnet3>,
        player2: Address<Testnet3>,
    ) -> Self {
        GameService {
            game_id,
            arbiter,
            players: (
                Player {
                    pubkey: player1,
                    state: PlayerState::Disconnected,
                    piece: None,
                    move_pos: None,
                },
                Player {
                    pubkey: player2,
                    state: PlayerState::Disconnected,
                    piece: None,
                    move_pos: None,
                },
            ),
            cur_player: player1,
        }
    }
}

// curl 'http://127.0.0.1:3000/join?pubkey=aleo17e9qgem7pvh44yw6takrrtvnf9m6urpmlwf04ytghds7d2dfdcpqtcy8cj&access_code=123'
async fn join(Query(query): Query<Join>, State(state): State<AppState>) -> impl IntoResponse {
    let Join {
        pubkey,
        access_code,
    } = query;
    let mut write_state = state.write().await;
    let usrs: Vec<_> = write_state
        .user_map
        .values()
        .filter(|u| u.access_code == access_code)
        .cloned()
        .collect();
    let arbiter = write_state.arbiter.1;

    match usrs.len() {
        2 => {
            if usrs[0].pubkey != pubkey && usrs[1].pubkey != pubkey {
                (
                    StatusCode::BAD_REQUEST,
                    Json(AppResponse::Error("access code used".into())),
                )
            } else {
                (
                    StatusCode::BAD_REQUEST,
                    Json(AppResponse::Error("game started".into())),
                )
            }
        }
        1 => {
            let game_id = if usrs[0].pubkey == pubkey {
                write_state
                    .user_map
                    .entry(pubkey)
                    .and_modify(|u| u.access_code = access_code);
                0
            } else {
                let game_id = Some(rand::random::<u64>());
                write_state.user_map.insert(
                    pubkey,
                    User {
                        pubkey,
                        access_code,
                        game_id,
                    },
                );
                write_state
                    .user_map
                    .entry(usrs[0].pubkey)
                    .and_modify(|u| u.game_id = game_id);

                let game_id = game_id.unwrap_or_default();
                let (tx, rx) = unbounded_channel();
                let game = Game {
                    players: (usrs[0].pubkey, pubkey),
                    tx,
                };
                let game_svc = GameService::new(game_id, arbiter, usrs[0].pubkey, pubkey);
                tokio::spawn({
                    let state = state.clone();
                    async {
                        game_svc.run(rx, state).await;
                    }
                });
                write_state.game_map.insert(game_id, game);
                game_id
            };
            (StatusCode::OK, Json(AppResponse::JoinResult { game_id }))
        }
        0 => {
            write_state.user_map.insert(
                pubkey,
                User {
                    pubkey,
                    access_code,
                    game_id: None,
                },
            );
            (StatusCode::OK, Json(AppResponse::JoinResult { game_id: 0 }))
        }
        _ => unreachable!(),
    }
}

// curl 'http://127.0.0.1:3000/join/aleo12m0ks7kd78ulf4669v2maynerc3jhj2ukkxyw6mdv6rag6xw8cpqdpm4vm'
async fn join_get(
    Path(pubkey): Path<Address<Testnet3>>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let state = state.read().await;

    if let Some(usr) = state.user_map.get(&pubkey) {
        (
            StatusCode::OK,
            Json(AppResponse::JoinResult {
                game_id: usr.game_id.unwrap_or_default(),
            }),
        )
    } else {
        (
            StatusCode::BAD_REQUEST,
            Json(AppResponse::Error("user not found".into())),
        )
    }
}

async fn enter_game(
    Query(query): Query<EnterGame>,
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let EnterGame { player, game_id } = query;
    let state = state.read().await;
    let game = state.game_map.get(&game_id);
    info!("enter game");
    if let Some(game) = game {
        {
            if game.players.0 != player && game.players.1 != player {
                return Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(body::boxed(body::Empty::new()))
                    .unwrap();
            }
        }
        let game_tx = game.tx.clone();
        drop(state);
        ws.on_upgrade(move |ws| handle_socket(ws, player, game_tx))
    } else {
        Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(body::boxed(body::Empty::new()))
            .unwrap()
    }
}

async fn handle_socket(ws: WebSocket, pubkey: Address<Testnet3>, game_tx: GameServiceSender) {
    async fn run(
        ws: WebSocket,
        pubkey: Address<Testnet3>,
        game_tx: GameServiceSender,
    ) -> eyre::Result<()> {
        let (ws_tx, mut ws_rx) = ws.split();
        let (tx, mut rx) = channel::<()>(1);
        let msg = GameServiceMsg::PlayerConnected(PlayerConn {
            pubkey,
            ws_tx,
            exit_signal: tx,
        });
        if let Err(e) = game_tx.send(msg) {
            bail!("send game service, error: {:?}", e);
        }

        loop {
            tokio::select! {
                Some(data) = ws_rx.next() => {
                    let data = data.wrap_err("recv")?;
                    if let Message::Text(data) = data {
                        info!("ws recving {}", data);
                        let msg: GameMessage = serde_json::from_str(&data).wrap_err("deserialize")?;
                        _ = game_tx.send(GameServiceMsg::GameMessage(pubkey, msg));
                    }
                }
                _ = rx.recv() => {
                    return Ok(());
                }
            }
        }
    }

    if let Err(e) = run(ws, pubkey, game_tx).await {
        error!("player ws, error: {:?}", e);
    }
}

fn banner() {
    let banner = indoc! {
        r#"

    __                    __   ____        __  __  __   
   / /   ____ _____  ____/ /  / __ )____ _/ /_/ /_/ /__ 
  / /   / __ `/ __ \/ __  /  / __  / __ `/ __/ __/ / _ \
 / /___/ /_/ / / / / /_/ /  / /_/ / /_/ / /_/ /_/ /  __/
/_____/\__,_/_/ /_/\__,_/  /_____/\__,_/\__/\__/_/\___/ 
                                                         
        "#
    };

    info!("{}", banner.green().bold());
}
