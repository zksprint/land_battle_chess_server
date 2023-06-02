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
use futures::{sink::SinkExt, stream::StreamExt};
use land_battle_chess_rs::setup_log_dispatch;
use log::info;
use serde::{Deserialize, Serialize};
use structopt::StructOpt;
use tokio::sync::{
    mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
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
    game_map: HashMap<String, Arc<RwLock<Game>>>,
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
    is_player2: bool,
    connected: bool,
    tx: Option<UnboundedSender<GameMessage>>,
}

struct Game {
    game_id: String,
    players: (Player, Player),
    cur_player: usize,
}

impl Game {
    fn opponent(&mut self, player: Address<Testnet3>) -> &mut Player {
        if self.players.0.pubkey == player {
            &mut self.players.1
        } else {
            &mut self.players.0
        }
    }

    fn self_player(&mut self, player: Address<Testnet3>) -> &mut Player {
        if self.players.0.pubkey == player {
            &mut self.players.0
        } else {
            &mut self.players.1
        }
    }

    fn new(game_id: String, player1: Address<Testnet3>, player2: Address<Testnet3>) -> Self {
        Game {
            game_id,
            players: (
                Player {
                    pubkey: player1,
                    is_player2: false,
                    connected: false,
                    tx: None,
                },
                Player {
                    pubkey: player2,
                    is_player2: true,
                    connected: false,
                    tx: None,
                },
            ),
            cur_player: 0,
        }
    }
}

#[derive(Debug, Deserialize)]
struct Join {
    access_code: String,
    pubkey: Address<Testnet3>,
}

#[derive(Serialize)]
enum AppResponse {
    Error(String),
    JoinResult { game_id: String },
}

// curl 'http://127.0.0.1:3000/join?pubkey=aleo17e9qgem7pvh44yw6takrrtvnf9m6urpmlwf04ytghds7d2dfdcpqtcy8cj&access_code=123'
async fn join(Query(query): Query<Join>, State(state): State<AppState>) -> impl IntoResponse {
    let Join {
        pubkey,
        access_code,
    } = query;
    let mut state = state.write().await;
    let usrs: Vec<_> = state
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
                state
                    .user_map
                    .entry(pubkey)
                    .and_modify(|u| u.access_code = access_code);
                String::new()
            } else {
                let game_id = Some(rand::random::<u64>().to_string());
                state.user_map.insert(
                    pubkey,
                    User {
                        pubkey,
                        access_code,
                        game_id: game_id.clone(),
                    },
                );
                state
                    .user_map
                    .entry(usrs[0].pubkey)
                    .and_modify(|u| u.game_id = game_id.clone());

                let game_id = game_id.clone().unwrap();
                let game = Arc::new(RwLock::new(Game::new(
                    game_id.clone(),
                    usrs[0].pubkey,
                    pubkey,
                )));
                state.game_map.insert(game_id.clone(), game);
                game_id
            };
            return (StatusCode::OK, Json(AppResponse::JoinResult { game_id }));
        }
        0 => {
            state.user_map.insert(
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
    if let Some(game) = game {
        {
            let game = game.read().await;
            if game.players.0.pubkey != query.player && game.players.1.pubkey != query.player {
                return Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(body::boxed(body::Empty::new()))
                    .unwrap();
            }
        }
        let game = game.clone();
        drop(state);
        ws.on_upgrade(move |ws| handle_socket(ws, game, player))
    } else {
        Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(body::boxed(body::Empty::new()))
            .unwrap()
    }
}

#[derive(Serialize, Deserialize)]
/*
json:
{"type":"role","game_id":1,"player1":"aleo12m0ks7kd78ulf4669v2maynerc3jhj2ukkxyw6mdv6rag6xw8cpqdpm4vm","player2":"aleo12m0ks7kd78ulf4669v2maynerc3jhj2ukkxyw6mdv6rag6xw8cpqdpm4vm"}
 */
#[serde(tag = "type", rename_all = "camelCase")]
enum GameMessage {
    OpponentDisconnected {
        // 对手下线
        game_id: String,
        pubkey: Address<Testnet3>,
    },
    OpponentConnected {
        game_id: String,
        pubkey: Address<Testnet3>,
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
        pubkey: Address<Testnet3>,
        piece: u64,
        x: u64, // 棋子坐标
        y: u32,
        target_x: u64, // 落子坐标
        target_y: u32,
    },
    PiecePos {
        // server 通知对手，行棋路线
        x: u64,
        y: u32,
        target_x: u64,
        target_y: u32,
    },
    Whisper {
        // 对手通知server，落子坐标棋子信息，如果piece 是司令，同时告知军棋坐标
        pubkey: Address<Testnet3>,
        piece: u64,
        x: u64,
        y: u32,
        flag_x: Option<u64>,
        flag_y: Option<u32>,
    },
}

async fn player_loop(
    ws: WebSocket,
    game: Arc<RwLock<Game>>,
    player: Address<Testnet3>,
) -> eyre::Result<()> {
    let (mut sender, _receiver) = ws.split();
    let (tx, rx) = unbounded_channel::<GameMessage>();

    let (opp_connected, game_id, player1, player2) = {
        let mut game = game.write().await;
        let self_player = game.self_player(player);
        self_player.connected = true;
        let opp = game.opponent(player);
        (
            opp.connected,
            game.game_id.clone(),
            game.players.0.pubkey,
            game.players.1.pubkey,
        )
    };

    sender
        .send(Message::Text(
            serde_json::to_string(&GameMessage::Role {
                game_id,
                player1,
                player2,
            })
            .unwrap(),
        ))
        .await;

    Ok(())
}

/// Actual websocket statemachine (one will be spawned per connection)
async fn handle_socket(ws: WebSocket, game: Arc<RwLock<Game>>, player: Address<Testnet3>) {
    player_loop(ws, game, player).await;
    /*
    loop {
        tokio::select! {
            Some(Ok(msg)) = receiver.next() => {
                match msg {
                    Message::Text(t) => {
                        let Ok(msg) = serde_json::from_str::<GameMessage>(&t) else {
                            return;
                        };
                    },
                    Message::Close(_) => {
                        return;
                    }
                    _ =>{}
                }
            },
            msg = rx.recv() => {

            }
        }
    }
    */
}

/// helper to print contents of messages to stdout. Has special treatment for Close.
fn process_message(msg: Message) -> eyre::Result<bool> {
    match msg {
        Message::Text(t) => {
            let _msg: GameMessage = serde_json::from_str(&t)?;
            Ok(true)
        }
        Message::Close(_c) => Ok(false),
        _ => Ok(true),
    }
}
