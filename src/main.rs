use std::convert::TryInto;
use std::{collections::HashMap, net::SocketAddr, path::PathBuf, str::FromStr, sync::Arc};

use aleo_rust::{Address, Testnet3};
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
use eyre::Context;
use futures::stream::SplitSink;
use futures::{sink::SinkExt, stream::StreamExt};
use land_battle_chess_rs::game_logic::{arb_piece, PieceInfo, INVALID_X, INVALID_Y};
use land_battle_chess_rs::{setup_log_dispatch, types::*};
use log::{error, info, warn};
use serde::Deserialize;
use structopt::StructOpt;
use tokio::sync::oneshot;
use tokio::sync::{
    mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
    oneshot::Sender,
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

    info!("land battle server running...");

    let app_state = AppState::default();
    let app = Router::new()
        .route("/join", get(join))
        .route("/join/:pubkey", get(join_get))
        .route("/game", get(enter_game))
        .layer(
            CorsLayer::new()
                .allow_origin("http://localhost:8000".parse::<HeaderValue>().unwrap())
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

#[derive(Default)]
struct App {
    user_map: HashMap<Address<Testnet3>, User>,
    game_map: HashMap<String, Game>,
}

type AppState = Arc<RwLock<App>>;

#[derive(Clone)]
struct User {
    pubkey: Address<Testnet3>,
    access_code: String,
    game_id: Option<String>,
}

struct Player {
    pubkey: Address<Testnet3>,
    connected: bool,
    piece: Option<PieceInfo>,
}

type GameServiceSender = UnboundedSender<(Address<Testnet3>, WebSocket, Sender<()>)>;

struct GameService {
    game_id: String,
    players: (Player, Player),
    cur_player: Address<Testnet3>,
}

#[derive(Debug)]
struct Game {
    players: (Address<Testnet3>, Address<Testnet3>),
    tx: GameServiceSender,
}

impl GameService {
    async fn run(
        mut self,
        mut rx: UnboundedReceiver<(Address<Testnet3>, WebSocket, Sender<()>)>,
        _app_state: AppState,
    ) {
        let (game_id, player1, player2) = (
            self.game_id.clone(),
            self.players.0.pubkey,
            self.players.1.pubkey,
        );
        let mut socket_map = HashMap::new();
        loop {
            tokio::select! {
                Some((addr, ws, exit)) = rx.recv() => {
                    match self.player_mut(addr) {
                        Some(player) => {
                            if player.connected {
                                todo!()
                            } else {
                                let (mut ws_tx, ws_rx) = ws.split();

                                if let Err(e) = ws_tx
                                    .send(
                                        GameMessage::Role {
                                            game_id: game_id.clone(),
                                            player1,
                                            player2,
                                        }
                                        .try_into().unwrap()
                                    )
                                    .await {
                                    warn!("[{}] send role to {}, error: {:?}", self.game_id, addr, e);
                                    continue;
                                }
                                player.connected = true;
                                socket_map.insert(addr, (ws_tx, ws_rx, exit));
                            }
                        },
                        None => {
                            ws.close().await;
                        }
                    }
                }
            }

            if socket_map.len() == 2 {
                break;
            }
        }

        let (mut ws_tx1, mut ws_rx1, _exit1) = socket_map.remove(&player1).unwrap();
        let (mut ws_tx2, mut ws_rx2, _exit2) = socket_map.remove(&player2).unwrap();
        let connected_msg: Message = GameMessage::Ready {
            game_id: self.game_id.clone(),
        }
        .try_into()
        .unwrap();
        ws_tx1.send(connected_msg.clone()).await;
        ws_tx2.send(connected_msg).await;

        'game_loop: loop {
            tokio::select! {
                Some(req) = ws_rx1.next() => {
                    match req {
                        Ok(msg) => {
                            if let Err(_e) = self.process_player_message(msg, player1, &mut ws_tx1, &mut ws_tx2).await {
                                break 'game_loop;
                            }
                        }
                        Err(e) => {
                            error!("recv msg error: {:?}", e);
                            break 'game_loop;
                        }
                    }
                }
                Some(req) = ws_rx2.next() => {
                    match req {
                        Ok(msg) => {
                            if let Err(_e) = self.process_player_message(msg, player2, &mut ws_tx2, &mut ws_tx1).await {
                                break 'game_loop;
                            }
                        }
                        Err(e) => {
                            error!("recv msg error: {:?}", e);
                            break 'game_loop;
                        }
                    }
                }
            }
        }
    }

    async fn process_player_message(
        &mut self,
        msg: Message,
        player: Address<Testnet3>,
        player_tx: &mut SplitSink<WebSocket, Message>,
        opp_tx: &mut SplitSink<WebSocket, Message>,
    ) -> eyre::Result<()> {
        let Message::Text(text) = msg else {
            return Ok(());
        };

        let game_id = self.game_id.clone();
        let msg: GameMessage = serde_json::from_str(&text).wrap_err("deserialize")?;
        match msg {
            GameMessage::Move {
                piece,
                x,
                y,
                target_x,
                target_y,
                flag_x,
                flag_y,
            } => {
                if self.cur_player != player {
                    warn!("[{}] not {} turn", game_id, player);
                    return Ok(());
                };

                let player = self.player_mut(player).unwrap();
                if player.piece.is_some() {
                    warn!("[{}] player:{} has piece", game_id, player.pubkey);
                    return Ok(());
                }

                player.piece = Some(PieceInfo {
                    piece,
                    x,
                    y,
                    flag_x: flag_x.unwrap_or(INVALID_X),
                    flag_y: flag_y.unwrap_or(INVALID_Y),
                });

                opp_tx
                    .send(
                        GameMessage::PiecePos {
                            x,
                            y,
                            target_x,
                            target_y,
                        }
                        .try_into()
                        .unwrap(),
                    )
                    .await
                    .wrap_err("send opp")?;
            }
            GameMessage::Whisper {
                piece,
                x,
                y,
                flag_x,
                flag_y,
            } => {
                if self.cur_player == player {
                    warn!("[{}] unexpect whisper from {}", game_id, player);
                    return Ok(());
                };

                let target = PieceInfo {
                    piece,
                    x,
                    y,
                    flag_x: flag_x.unwrap_or(INVALID_X),
                    flag_y: flag_y.unwrap_or(INVALID_Y),
                };
                let player = self.player_mut(player).unwrap();
                let attacker = player.piece.take().unwrap();
                let piece_move = arb_piece(attacker, target);

                let msg: Message = GameMessage::MoveResult(piece_move).try_into().unwrap();
                player_tx.send(msg.clone()).await;
                opp_tx.send(msg).await;
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

    fn player(&self, player: Address<Testnet3>) -> Option<&Player> {
        if self.players.0.pubkey == player {
            Some(&self.players.0)
        } else if self.players.1.pubkey == player {
            Some(&self.players.1)
        } else {
            None
        }
    }

    fn player_mut(&mut self, player: Address<Testnet3>) -> Option<&mut Player> {
        if self.players.0.pubkey == player {
            Some(&mut self.players.0)
        } else if self.players.1.pubkey == player {
            Some(&mut self.players.1)
        } else {
            None
        }
    }

    fn new(game_id: String, player1: Address<Testnet3>, player2: Address<Testnet3>) -> Self {
        GameService {
            game_id,
            players: (
                Player {
                    pubkey: player1,
                    connected: false,
                    piece: None,
                },
                Player {
                    pubkey: player2,
                    connected: false,
                    piece: None,
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

    match usrs.len() {
        2 => {
            if usrs[0].pubkey != pubkey && usrs[1].pubkey != pubkey {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(AppResponse::Error("access code used".into())),
                );
            } else {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(AppResponse::Error("game started".into())),
                );
            }
        }
        1 => {
            let game_id = if usrs[0].pubkey == pubkey {
                write_state
                    .user_map
                    .entry(pubkey)
                    .and_modify(|u| u.access_code = access_code);
                String::new()
            } else {
                let game_id = Some(rand::random::<u64>().to_string());
                write_state.user_map.insert(
                    pubkey,
                    User {
                        pubkey,
                        access_code,
                        game_id: game_id.clone(),
                    },
                );
                write_state
                    .user_map
                    .entry(usrs[0].pubkey)
                    .and_modify(|u| u.game_id = game_id.clone());

                let game_id = game_id.clone().unwrap();
                let (tx, rx) = unbounded_channel();
                let game = Game {
                    players: (usrs[0].pubkey, pubkey),
                    tx,
                };
                let game_svc = GameService::new(game_id.clone(), usrs[0].pubkey, pubkey);
                tokio::spawn({
                    let state = state.clone();
                    async {
                        game_svc.run(rx, state).await;
                    }
                });
                write_state.game_map.insert(game_id.clone(), game);
                game_id
            };
            return (StatusCode::OK, Json(AppResponse::JoinResult { game_id }));
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
            return (
                StatusCode::OK,
                Json(AppResponse::JoinResult {
                    game_id: "0".into(),
                }),
            );
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
        return (
            StatusCode::OK,
            Json(AppResponse::JoinResult {
                game_id: usr.game_id.clone().unwrap_or("0".into()),
            }),
        );
    } else {
        return (
            StatusCode::BAD_REQUEST,
            Json(AppResponse::Error("user not found".into())),
        );
    }
}

#[derive(Debug, Deserialize)]
struct EnterGame {
    player: Address<Testnet3>,
    game_id: String,
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
            info!("game:{:?}, player:{}", game, player);
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
        info!("game_id:{:?} not found", game_id);
        Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(body::boxed(body::Empty::new()))
            .unwrap()
    }
}

async fn handle_socket(ws: WebSocket, pubkey: Address<Testnet3>, game_tx: GameServiceSender) {
    let (tx, rx) = oneshot::channel();
    if let Err(e) = game_tx.send((pubkey, ws, tx)) {
        error!("send game service, error: {:?}", e);
        return;
    }
    if let Err(e) = rx.await {
        error!("wait exit signal, error: {:?}", e);
        return;
    };
}
