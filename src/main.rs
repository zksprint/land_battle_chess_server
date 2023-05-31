use std::{collections::HashMap, net::SocketAddr, str::FromStr, sync::Arc};

use aleo_rust::{Address, Testnet3};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let app_state = AppState::default();
    let app = Router::new()
        .route("/join", post(join))
        .route("/join/:pubkey", get(join_get))
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
    game_map: HashMap<u64, Game>,
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
}

struct Game {
    game_id: u64,
    players: (Player, Player),
    cur_player: usize,
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

// curl -X POST 'http://127.0.0.1:3000/join' -H 'Content-Type: application/json' -d '{"pubkey":"aleo17e9qgem7pvh44yw6takrrtvnf9m6urpmlwf04ytghds7d2dfdcpqtcy8cj","access_code":"123"}'
async fn join(State(state): State<AppState>, Json(input): Json<Join>) -> impl IntoResponse {
    let Join {
        pubkey,
        access_code,
    } = input;
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
                game_id.unwrap_or_default()
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
    /*
    let Ok(pubkey) = Address::<Testnet3>::from_str(&pubkey) else {
        return (StatusCode::BAD_REQUEST, Json(AppResponse::Error("invalid pubkey".into())));
    };
    */

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

/*
#[derive(Serialize)]
enum GameMessage {
    Move { pubkey: A },
} */
