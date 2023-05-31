use std::{collections::HashMap, net::SocketAddr, str::FromStr, sync::Arc};

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
use futures::stream::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::{
    mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
    RwLock,
};

use tower_http::cors::CorsLayer;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let app_state = AppState::default();
    let app = Router::new()
        .route("/join", get(join))
        .route("/join/:pubkey", get(join_get))
        .route("/game/:game_id", get(enter_game))
        .layer(
            CorsLayer::new()
                .allow_origin("http://localhost:8000".parse::<HeaderValue>().unwrap())
                .allow_methods([Method::GET, Method::POST]),
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
    game_map: HashMap<u64, Arc<RwLock<Game>>>,
}

type AppState = Arc<RwLock<App>>;

#[derive(Clone)]
struct User {
    pubkey: Address<Testnet3>,
    access_code: String,
    game_id: Option<u64>,
}

struct Player {
    pubkey: Address<Testnet3>,
    is_player2: bool,
    rx: Option<UnboundedReceiver<GameMessage>>,
    tx: UnboundedSender<GameMessage>,
}

struct Game {
    game_id: u64,
    players: (Player, Player),
    cur_player: usize,
}

impl Game {
    fn opponent(&self, player: Address<Testnet3>) -> &Player {
        if self.players.0.pubkey == player {
            &self.players.1
        } else {
            &self.players.0
        }
    }

    fn self_player(&self, player: Address<Testnet3>) -> &Player {
        if self.players.0.pubkey == player {
            &self.players.0
        } else {
            &self.players.1
        }
    }

    fn new(game_id: u64, player1: Address<Testnet3>, player2: Address<Testnet3>) -> Self {
        let (tx1, rx1) = unbounded_channel::<GameMessage>();
        let (tx2, rx2) = unbounded_channel::<GameMessage>();
        Game {
            game_id,
            players: (
                Player {
                    pubkey: player1,
                    is_player2: false,
                    rx: Some(rx1),
                    tx: tx1,
                },
                Player {
                    pubkey: player2,
                    is_player2: true,
                    rx: Some(rx2),
                    tx: tx2,
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
    JoinResult { game_id: u64 },
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
                0
            } else {
                let game_id = Some(rand::random::<u64>());
                state.user_map.insert(
                    pubkey,
                    User {
                        pubkey,
                        access_code,
                        game_id,
                    },
                );
                state
                    .user_map
                    .entry(usrs[0].pubkey)
                    .and_modify(|u| u.game_id = game_id);

                let game_id = game_id.unwrap();
                let game = Arc::new(RwLock::new(Game::new(game_id, usrs[0].pubkey, pubkey)));
                state.game_map.insert(game_id, game);
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
            return (StatusCode::OK, Json(AppResponse::JoinResult { game_id: 0 }));
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
                game_id: usr.game_id.unwrap_or_default(),
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
    game_id: u64,
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
enum GameMessage {
    Move { pubkey: Address<Testnet3> },
}

/// Actual websocket statemachine (one will be spawned per connection)
async fn handle_socket(ws: WebSocket, _game: Arc<RwLock<Game>>, _player: Address<Testnet3>) {
    let (_sender, _receiver) = ws.split();
    /*
    let opp = game.opponent(player);
    let self_player = game.self_player(player);
    let mut rx = self_player.rx.take().unwrap();
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
