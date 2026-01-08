use std::collections::{HashMap, HashSet};

use crate::error::{BtcError, Result};
use crate::sha256::Hash;
use crate::util::MerkleRoot;
use crate::{
    crypto::{PublicKey, Signature},
    U256,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Blockchain {
    pub utxos: HashMap<Hash, TransactionOutput>,
    pub target: U256,
    pub blocks: Vec<Block>,
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

    pub fn block_height(&self) -> u64 {
        self.blocks.len() as u64
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
            let calculated_merkle_root = MerkleRoot::calculate(&block.transactions);
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
                    self.utxos.insert(transaction.hash(), output.clone());
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
        let target_seconds = crate::IDEAL_BLOCK_TIME * crate::DIFFICULTY_UPDATE_INTERVAL;

        // target * (실제 시간 / 기대시간)
        // 너무 빨리 되었다면 (실제 시간 / 기대시간) < 1 -> target이 더 어려워지게
        // 너무 느리게 되었다면 (실제 시간 / 기대 시간) > 1 -> target이 더 쉬워지게
        let new_target = self.target * (time_diff_seconds as f64 / target_seconds as f64) as usize;

        // 현재 난이도의 25%, 400% 내에서만 움직이도록 clamp 처리한다. 너무 급격한 난이도 변경을 방지.
        let new_target = if new_target < self.target / 4 {
            self.target / 4
        } else if new_target > self.target * 4 {
            self.target * 4
        } else {
            new_target
        };

        // 최소보다는 커야 하므로
        self.target = new_target.min(crate::MIN_TARGET);
    }
}

// --------------------------------------------
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Block {
    pub header: BlockHeader,
    pub transactions: Vec<Transaction>,
}

impl Block {
    pub fn new(header: BlockHeader, transactions: Vec<Transaction>) -> Self {
        Self {
            header,
            transactions,
        }
    }

    pub fn hash(&self) -> Hash {
        Hash::hash(self)
    }

    pub fn calculate_miner_fees(&self, utxos: &HashMap<Hash, TransactionOutput>) -> Result<u64> {
        let mut inputs: HashMap<Hash, TransactionOutput> = HashMap::new();
        let mut outputs: HashMap<Hash, TransactionOutput> = HashMap::new();

        for transaction in self.transactions.iter().skip(1) {
            // input
            for input in &transaction.inputs {
                let prev_output = utxos.get(&input.prev_transaction_output_hash);
                if prev_output.is_none() {
                    return Err(BtcError::InvalidTransaction);
                }
                let prev_output = prev_output.unwrap();
                if inputs.contains_key(&input.prev_transaction_output_hash) {
                    return Err(BtcError::InvalidTransaction);
                }
                inputs.insert(input.prev_transaction_output_hash, prev_output.clone());
            }

            // output
            for output in &transaction.outputs {
                if outputs.contains_key(&output.hash()) {
                    return Err(BtcError::InvalidTransaction);
                }
                outputs.insert(output.hash(), output.clone());
            }
        }

        let input_value: u64 = inputs.values().map(|output| output.value).sum();
        let output_value: u64 = outputs.values().map(|output| output.value).sum();
        Ok(input_value - output_value)
    }

    pub fn verify_coinbase_transaction(
        &self,
        predicted_block_height: u64,
        utxos: &HashMap<Hash, TransactionOutput>,
    ) -> Result<()> {
        let coinbase_transaction = &self.transactions[0];

        if coinbase_transaction.inputs.len() != 0 {
            return Err(BtcError::InvalidTransaction);
        }
        if coinbase_transaction.outputs.len() == 0 {
            return Err(BtcError::InvalidTransaction);
        }

        // 사용자들이 낸 수수료
        let miner_fees = self.calculate_miner_fees(utxos)?;

        // 보상 * 사토시 변환 / 반감기에 따른 2승수 나눗셈
        let block_reward = crate::INITIAL_REWARD * 10u64.pow(8)
            / 2u64.pow((predicted_block_height / crate::HALVING_INTERVAL) as u32);

        // coinbase tx의 출력값의 합은 블록 보상과 miner fee의 합과 동일하다.
        let total_coinbase_outputs: u64 =
            coinbase_transaction.outputs.iter().map(|output| output.value).sum();

        if total_coinbase_outputs != block_reward + miner_fees {
            return Err(BtcError::InvalidTransaction);
        }

        Ok(())
    }

    pub fn verify_transactions(
        &self,
        predicted_block_height: u64,
        utxos: &HashMap<Hash, TransactionOutput>,
    ) -> Result<()> {
        // 해당 블록 내 소비될 utxo
        // 같은 블록 내 이중 지출을 막기 위한 로컬 변수
        let mut inputs: HashMap<Hash, TransactionOutput> = HashMap::new();

        // tx를 하나도 안 들고 있는 블록 처리
        if self.transactions.is_empty() {
            return Err(BtcError::InvalidTransaction);
        }

        self.verify_coinbase_transaction(predicted_block_height, utxos)?;

        // 일반적인 tx 검증. except coinbase (first tx)
        for transaction in self.transactions.iter().skip(1) {
            let mut input_value = 0;
            let mut output_value = 0;

            // input 검증
            for input in &transaction.inputs {
                // input 해시가 참조하는 이전 tx
                let prev_output = utxos.get(&input.prev_transaction_output_hash);
                if prev_output.is_none() {
                    return Err(BtcError::InvalidTransaction);
                }
                let prev_output = prev_output.unwrap();

                // double-spending 방지
                // 로컬 변수인 inputs 상에 누적된 input들 중 이전 tx 중 사용된 것이 하나라도 있으면 그것은 이중 지출이므로 걸러낸다.
                if inputs.contains_key(&input.prev_transaction_output_hash) {
                    return Err(BtcError::InvalidTransaction);
                }

                // input으로 사용될 tx의 이전 output이 올바른 소유자에 의해 서명된 것인지 확인
                if !input.signature.verify(&input.prev_transaction_output_hash, &prev_output.pubkey)
                {
                    return Err(BtcError::InvalidSignature);
                }
                input_value += prev_output.value;
                inputs.insert(input.prev_transaction_output_hash, prev_output.clone());
            }

            // output 처리
            for output in &transaction.outputs {
                output_value += output.value;
            }

            // 채굴 보상이 있으므로 output 값어치는 input 값어치보다 항상 적어야 한다.
            if input_value < output_value {
                return Err(BtcError::InvalidTransaction);
            }
        }

        Ok(())
    }
}

// --------------------------------------------
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BlockHeader {
    pub timestamp: DateTime<Utc>,
    pub nonce: u64,
    pub prev_block_hash: Hash,
    /// tx aggregated to single merkle root
    pub merkle_root: MerkleRoot,
    /// for POW
    pub target: U256,
}

impl BlockHeader {
    pub fn new(
        timestamp: DateTime<Utc>,
        nonce: u64,
        prev_block_hash: Hash,
        merkle_root: MerkleRoot,
        target: U256,
    ) -> Self {
        Self {
            timestamp,
            nonce,
            prev_block_hash,
            merkle_root,
            target,
        }
    }

    pub fn hash(&self) -> Hash {
        Hash::hash(self)
    }
}

// --------------------------------------------
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Transaction {
    pub inputs: Vec<TransactionInput>,
    pub outputs: Vec<TransactionOutput>,
}

impl Transaction {
    pub fn new(inputs: Vec<TransactionInput>, outputs: Vec<TransactionOutput>) -> Self {
        Transaction {
            inputs,
            outputs,
        }
    }
    pub fn hash(&self) -> Hash {
        Hash::hash(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TransactionInput {
    /// input으로 사용할 이전 output tx.
    pub prev_transaction_output_hash: Hash,
    pub signature: Signature,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TransactionOutput {
    pub value: u64,
    pub unique_id: Uuid,
    pub pubkey: PublicKey,
}

impl TransactionOutput {
    pub fn hash(&self) -> Hash {
        Hash::hash(self)
    }
}
