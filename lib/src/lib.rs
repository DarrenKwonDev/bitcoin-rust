use serde::{Deserialize, Serialize};
use uint::construct_uint;

pub mod crypto;
pub mod error;
pub mod network;
pub mod sha256;
pub mod types;
pub mod util;

construct_uint! {
    #[derive(Serialize, Deserialize)]
    pub struct U256(4);
}

// 채굴 보상. 50 × 10^8 = 5,000,000,000 satoshis
pub const INITIAL_REWARD: u64 = 50;

// 반감기 (실제 bitcoin은 210,000)
pub const HALVING_INTERVAL: u64 = 210;

// 블록 생성 시간 목표치 10초. 실제 시간과 비교하여 난이도를 조정하는데 활용
pub const IDEAL_BLOCK_TIME: u64 = 10;

// minimum target
// 0x0000FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF
pub const MIN_TARGET: U256 = U256([
    0xFFFF_FFFF_FFFF_FFFF,
    0xFFFF_FFFF_FFFF_FFFF,
    0xFFFF_FFFF_FFFF_FFFF,
    0x0000_FFFF_FFFF_FFFF,
]);

// 난이도 조정 (실제 bitcoin은 2016 블록마다 조정)
pub const DIFFICULTY_UPDATE_INTERVAL: u64 = 50;

// 600 블록이 지나도 mempool에서 소비되지 않으면 tx를 버린다
pub const MAX_MEMPOOL_TRANSACTION_AGE: u64 = 600;

// 블록당 최대 20개의 블록만 허용
pub const BLOCK_TRANSACTION_CAP: usize = 20;
