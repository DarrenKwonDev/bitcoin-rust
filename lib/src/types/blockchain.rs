use crate::error::{BtcError, Result};
use crate::sha256::Hash;
use crate::types::block::Block;
use crate::types::transaction::{Transaction, TransactionOutput};
use crate::util::{MerkleRoot, Savable};
use crate::U256;
use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::{
    Error as IoError, ErrorKind as IoErrorKind, Read, Result as IoResult, Write,
};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Blockchain {
    // mark(true) 라면 해당 utxo가 현재 mempool의 다른 트랜잭션에서 사용 중인지
    utxos: HashMap<Hash, (bool, TransactionOutput)>,
    target: U256,
    blocks: Vec<Block>,
    #[serde(default, skip_serializing)]
    mempool: Vec<(DateTime<Utc>, Transaction)>,
}

impl Blockchain {
    pub fn new() -> Self {
        Blockchain {
            utxos: HashMap::new(),
            target: crate::MIN_TARGET,
            blocks: vec![],
            mempool: vec![],
        }
    }

    // utxos getter
    pub fn utxos(&self) -> &HashMap<Hash, (bool, TransactionOutput)> {
        &self.utxos
    }
    // target getter
    pub fn target(&self) -> U256 {
        self.target
    }
    // blocks getter
    pub fn blocks(&self) -> impl Iterator<Item = &Block> {
        self.blocks.iter()
    }
    // mempool getter
    pub fn mempool(&self) -> &[(DateTime<Utc>, Transaction)] {
        &self.mempool
    }

    pub fn block_height(&self) -> u64 {
        self.blocks.len() as u64
    }

    pub fn calculate_block_reward(&self) -> u64 {
        let block_height = self.block_height();
        let halvings = block_height / crate::HALVING_INTERVAL;

        if halvings >= 64 {
            // After 64 halvings, the reward becomes 0
            0
        } else {
            (crate::INITIAL_REWARD * 10u64.pow(8)) >> halvings
        }
    }

    // 외부에서 전송 받은 tx를 mempool에 추가한다.
    pub fn add_to_mempool(&mut self, transaction: Transaction) -> Result<()> {
        let mut known_inputs = HashSet::new();

        for input in &transaction.inputs {
            // input이 유래한 output이 utxo에 존재해야만 한다.
            if !self.utxos.contains_key(&input.prev_transaction_output_hash) {
                return Err(BtcError::InvalidTransaction);
            }
            // utxo의 이중 사용은 불가하므로 이미 set에 존재한다면 바른 tx가 아니다.
            if known_inputs.contains(&input.prev_transaction_output_hash) {
                return Err(BtcError::InvalidTransaction);
            }

            // utxo의 소비한 output hash를 inputs에 넣는다.
            known_inputs.insert(input.prev_transaction_output_hash);
        }

        // -----------------------------------
        // RBF (Replace-By-Fee) 로직
        // 원래라면 실제 비트코인에서는 수수료 비교해서 miner fee가 더 나오는 것을 선택함.
        // 여기서는 단순하게 나중에 온 것을 우선시하고, 이전에 있던 건 mempool에서 삭제

        // 이 utxo가 이미 mempool의 다른 트랜잭션에서 사용 중이면
        // 그 트랜잭션을 찾아서 제거하고
        // 그 트랜잭션이 사용한 모든 utxo의 마킹을 해제
        for input in &transaction.inputs {
            // 이미 사용된 output이 utxo에 존재하는 경우, 이중 사용된 output임.
            if let Some((true, _)) =
                self.utxos.get(&input.prev_transaction_output_hash)
            {
                // 해당 utxo를 사용한, 먼저 mempool에 있던 tx를 찾아냄
                let referencing_transaction = self
                    .mempool
                    .iter()
                    .enumerate()
                    .find(|(_, (_, transaction))| {
                        transaction.outputs.iter().any(|output| {
                            output.hash() == input.prev_transaction_output_hash
                        })
                    });

                // 지워야 할 기존 tx가 사용한 input들을 모두 사용 가능한 형태(mark=false) 로 되돌린다.
                if let Some((idx, (_, referencing_transaction))) =
                    referencing_transaction
                {
                    for input in &referencing_transaction.inputs {
                        self.utxos
                            .entry(input.prev_transaction_output_hash)
                            .and_modify(|(marked, _)| {
                                *marked = false;
                            });
                    }

                    // remove the transaction from the mempool
                    self.mempool.remove(idx);
                } else {
                    // 분명 이중 사용된 utxo이었을 텐데, 그걸 사용한 기존 tx를 mempool에서 발견하지 못했다?
                    // 이상한 케이스가 맞지만 해당 utxo의 mark를 false (아직 사용되지 않음) 으로 바꾼다
                    self.utxos
                        .entry(input.prev_transaction_output_hash)
                        .and_modify(|(marked, _)| {
                            *marked = false;
                        });
                }
            }
        }

        // -----------------------------------
        // input이 활용한 이전 block의 output value를 모두 모은다
        let all_inputs = transaction
            .inputs
            .iter()
            .map(|input| {
                self.utxos
                    .get(&input.prev_transaction_output_hash)
                    .expect("BUG: impossible")
                    .1
                    .value
            })
            .sum::<u64>();

        // 결과로 생성된 이번 블록의 output value를 더한다.
        let all_outputs =
            transaction.outputs.iter().map(|output| output.value).sum::<u64>();

        // 수수료를 생각하면 input이 항상 output보다 커야 한다
        if all_inputs < all_outputs {
            return Err(BtcError::InvalidTransaction);
        }

        // -----------------------------------
        // mempool에 tx를 추가한다
        self.mempool.push((Utc::now(), transaction));

        // miner fee를 maximize하기 위해서 정렬한다
        self.mempool.sort_by_key(|(_, transaction)| {
            let all_inputs = transaction
                .inputs
                .iter()
                .map(|input| {
                    self.utxos
                        .get(&input.prev_transaction_output_hash)
                        .expect("BUG: impossible")
                        .1
                        .value
                })
                .sum::<u64>();

            let all_outputs = transaction
                .outputs
                .iter()
                .map(|output| output.value)
                .sum::<u64>();

            let miner_fee = all_inputs - all_outputs;
            miner_fee
        });

        Ok(())
    }

    pub fn cleanup_mempool(&mut self) {
        let now = Utc::now();
        let mut utxo_hashes_to_unmark: Vec<Hash> = vec![];

        // 시간 지났으면 지워야 할 tx가 소비했던 input utxo들을 저장해뒀다가 mark=false로 바꾼다
        self.mempool.retain(|(timestamp, transaction)| {
            if now - *timestamp
                > chrono::Duration::seconds(
                    crate::MAX_MEMPOOL_TRANSACTION_AGE as i64,
                )
            {
                utxo_hashes_to_unmark.extend(
                    transaction
                        .inputs
                        .iter()
                        .map(|input| input.prev_transaction_output_hash),
                );
                false
            } else {
                true
            }
        });

        for hash in utxo_hashes_to_unmark {
            self.utxos.entry(hash).and_modify(|(marked, _)| {
                *marked = false;
            });
        }
    }

    pub fn add_block(&mut self, block: Block) -> Result<()> {
        // 체인에 블록이 하나도 없다면
        if self.blocks.is_empty() {
            // 제네시스 블록의 prev는 zero hash여야만 한다
            if block.header.prev_block_hash != Hash::zero() {
                println!("zero hash");
                return Err(BtcError::InvalidBlock);
            }
        } else {
            // 새 블록의 prev block hash는 이전 블록 해시와 일치해야 한다
            let last_block = self.blocks.last().unwrap();

            // 블록체인 상 마지막 블록의 해시는 현재 채굴된 블록의 prev_block_hash와 동일해야 한다
            if block.header.prev_block_hash != last_block.hash() {
                println!("prev hash is wrong");
                return Err(BtcError::InvalidBlock);
            }

            // 현재 채굴된 block은 지정된 target보다는 커야 한다
            if !block.header.hash().matches_target(block.header.target) {
                println!("does not match target");
                return Err(BtcError::InvalidBlock);
            }

            // merkel root가 바르게 계산되었는지 체크한다 (tx 변조, 추가, 누락 여부 확인)
            let calculated_merkle_root =
                MerkleRoot::calculate(&block.transactions);
            if calculated_merkle_root != block.header.merkle_root {
                println!("invalid merkle root");
                return Err(BtcError::InvalidMerkleRoot);
            }

            // 채굴된 시간이 마지막 블록 채굴된 시간 이후여야 한다
            if block.header.timestamp <= last_block.header.timestamp {
                return Err(BtcError::InvalidBlock);
            }

            // 각 block이 포함한 tx를 다양한 형태로 검증한다.
            block.verify_transactions(self.block_height(), &self.utxos)?;
        }

        // 채굴된 블록의 tx를 모아서 mempool에서 지운다 (처리된 것이므로)
        let block_transactions: HashSet<_> =
            block.transactions.iter().map(|tx| tx.hash()).collect();
        self.mempool.retain(|(_, tx)| !block_transactions.contains(&tx.hash()));

        self.blocks.push(block);

        self.try_adjust_target();

        Ok(())
    }

    // quite inefficient, but for simplicitiy.
    pub fn rebuild_utxos(&mut self) {
        for block in &self.blocks {
            for transaction in &block.transactions {
                for input in &transaction.inputs {
                    self.utxos.remove(&input.prev_transaction_output_hash);
                }
                for output in transaction.outputs.iter() {
                    self.utxos
                        .insert(transaction.hash(), (false, output.clone()));
                }
            }
        }
    }

    pub fn try_adjust_target(&mut self) {
        if self.blocks.is_empty() {
            return;
        }
        if self.blocks.len() % crate::DIFFICULTY_UPDATE_INTERVAL as usize != 0 {
            return;
        }

        // 현재보다 50개 이전의 timestamp
        let start_time = self.blocks
            [self.blocks.len() - crate::DIFFICULTY_UPDATE_INTERVAL as usize]
            .header
            .timestamp;
        let end_time = self.blocks.last().unwrap().header.timestamp;

        // 50개 블록이 만들어질 때 까지 걸린 시간
        let time_diff = end_time - start_time;
        let time_diff_seconds = time_diff.num_seconds();

        // 이전 50개의 블록이 생성된 시간이 IDLE한 blocktime과 얼마나 차이가 났는지?
        let target_seconds =
            crate::IDEAL_BLOCK_TIME * crate::DIFFICULTY_UPDATE_INTERVAL;

        // 실제 bitcoin에서는 leading zero 의 갯수를 늘려서 난이도를 증가 시킴.
        // 여기서는 간이적으로 처리
        // target * (실제 시간 / 기대시간)
        // 너무 빨리 되었다면 (실제 시간 / 기대시간) < 1 -> target이 더 어려워지게 (target이 낮아질수록 조건을 만족하는 해시 만들기가 어려움)
        // 너무 느리게 되었다면 (실제 시간 / 기대 시간) > 1 -> target이 더 쉬워지게
        let new_target =
            BigDecimal::parse_bytes(&self.target.to_string().as_bytes(), 10)
                .expect("BUG: impossible")
                * (BigDecimal::from(time_diff_seconds)
                    / BigDecimal::from(target_seconds));

        // cut off decimal point and everything after
        // it from string representation of new_target
        let new_target_str = new_target
            .to_string()
            .split('.')
            .next()
            .expect("BUG: Expected a decimal point")
            .to_owned();

        let new_target: U256 =
            U256::from_str_radix(&new_target_str, 10).expect("BUG: impossible");

        dbg!(new_target);

        // 현재 난이도의 25%, 400% 내에서만 움직이도록 clamp 처리한다. 너무 급격한 난이도 변경을 방지.
        let new_target = if new_target < self.target / 4 {
            dbg!(self.target / 4)
        } else if new_target > self.target * 4 {
            dbg!(self.target * 4)
        } else {
            new_target
        };

        dbg!(new_target);

        // 최소보다는 커야 하므로
        self.target = new_target.min(crate::MIN_TARGET);
        dbg!(self.target);
    }
}

impl Savable for Blockchain {
    fn load<I: Read>(reader: I) -> IoResult<Self> {
        ciborium::de::from_reader(reader).map_err(|_| {
            IoError::new(
                IoErrorKind::InvalidData,
                "Failed to deseriailize blockchain",
            )
        })
    }

    fn save<O: Write>(&self, writer: O) -> IoResult<()> {
        ciborium::ser::into_writer(self, writer).map_err(|_| {
            IoError::new(
                IoErrorKind::InvalidData,
                "Failed to serialize blockchain",
            )
        })
    }
}
