use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Miner {
    pub id: i64,
    pub address: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Worker {
    pub id: i64,
    pub miner_id: i64,
    pub name: String,
    pub last_seen: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Share {
    pub id: i64,
    pub worker_id: i64,
    pub job_id: String,
    pub difficulty: f64,
    pub is_block: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Block {
    pub id: i64,
    pub height: i64,
    pub hash: String,
    pub reward: i64,
    pub status: String,
    pub found_by: Option<i64>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Balance {
    pub miner_id: i64,
    pub pending: i64,
    pub paid: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Payout {
    pub id: i64,
    pub miner_id: i64,
    pub txid: Option<String>,
    pub amount: i64,
    pub created_at: String,
}
