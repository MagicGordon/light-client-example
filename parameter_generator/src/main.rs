use std::str::FromStr;
use std::{error::Error, fmt};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use bitcoin::hashes::Hash;
use bitcoin::{consensus::deserialize, Block};
use reqwest::Client;
use serde::de::{self, Visitor};
use serde::{Deserialize, Serialize};
use borsh::{BorshDeserialize, BorshSerialize};
use clap::Parser;

#[derive(BorshDeserialize, BorshSerialize, Clone)]
pub struct H256(pub [u8; 32]);

impl FromStr for H256 {
    type Err = hex::FromHexError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut result = [0; 32];
        hex::decode_to_slice(s, &mut result)?;
        result.reverse();
        Ok(H256(result))
    }
}

impl fmt::Display for H256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reversed: Vec<u8> = self.0.into_iter().rev().collect();
        write!(f, "{}", hex::encode(reversed))
    }
}


#[derive(BorshDeserialize, BorshSerialize)]
pub struct ProofArgs {
    pub tx_id: H256,
    pub tx_block_blockhash: H256,
    pub tx_index: u64,
    pub merkle_proof: Vec<H256>,
    pub confirmations: u64,
}

impl<'de> Deserialize<'de> for H256 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct HexVisitor;

        impl<'de> Visitor<'de> for HexVisitor {
            type Value = H256;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a hex string")
            }

            fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let mut result = [0; 32];
                hex::decode_to_slice(s, &mut result).map_err(de::Error::custom)?;
                result.reverse();
                Ok(H256(result))
            }
        }

        deserializer.deserialize_str(HexVisitor)
    }
}

impl Serialize for H256 {
    fn serialize<S>(
        &self,
        serializer: S,
    ) -> Result<<S as serde::Serializer>::Ok, <S as serde::Serializer>::Error>
    where
        S: serde::Serializer,
    {
        let reversed: Vec<u8> = self.0.into_iter().rev().collect();
        serializer.serialize_str(&hex::encode(reversed))
    }
}

async fn get_block_by_tx_hash(tx_hash: &str) -> Result<Block, Box<dyn Error>> {
    let client = Client::new();
    let url = format!("https://blockstream.info/testnet/api/tx/{}", tx_hash);
    let tx_json = client.get(&url).send().await?.json::<serde_json::Value>().await?;
    let block_hash = tx_json["status"]["block_hash"].as_str().unwrap();

    let url = format!("https://blockstream.info/testnet/api/block/{}/raw", block_hash);
    let block_bytes = client.get(&url).send().await?.bytes().await?.to_vec();
    let block: Block = deserialize(&block_bytes)?;
    Ok(block)
}

pub fn merkle_proof_calculator(tx_hashes: Vec<H256>, transaction_position: usize) -> Vec<H256> {
    let mut transaction_position = transaction_position;
    let mut merkle_proof = Vec::new();
    let mut current_hashes = tx_hashes;

    while current_hashes.len() > 1 {
        if current_hashes.len() % 2 == 1 {
            current_hashes.push(current_hashes[current_hashes.len() - 1].clone())
        }

        if transaction_position % 2 == 1 {
            merkle_proof.push(current_hashes[transaction_position - 1].clone());
        } else {
            merkle_proof.push(current_hashes[transaction_position + 1].clone());
        }

        let mut new_hashes = Vec::new();

        for i in (0..current_hashes.len() - 1).step_by(2) {
            new_hashes.push(compute_hash(&current_hashes[i], &current_hashes[i + 1]));
        }

        current_hashes = new_hashes;
        transaction_position /= 2;
    }

    merkle_proof
}

fn compute_hash(first_tx_hash: &H256, second_tx_hash: &H256) -> H256 {
    let mut concat_inputs = Vec::with_capacity(64);
    concat_inputs.extend(first_tx_hash.0);
    concat_inputs.extend(second_tx_hash.0);

    double_sha256(&concat_inputs)
}

pub fn double_sha256(input: &[u8]) -> H256 {
    use sha2::{Digest, Sha256};
    H256(Sha256::digest(Sha256::digest(input)).into())
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long)]
    tx_id: String,

    #[arg(long)]
    confirmations: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let tx_id = &args.tx_id;
    let confirmations = args.confirmations;
    let block = get_block_by_tx_hash(tx_id).await?;
    let block_hash = block.header.block_hash().to_string();
    println!("tx_id: {:?}", tx_id);
    println!("confirmations: {:?}", confirmations);
    println!("tx_block_blockhash: {:?}", block_hash);
    let transactions = block
        .txdata
        .iter()
        .map(|tx| H256(tx.compute_txid().to_byte_array()))
        .collect::<Vec<_>>();
    let transaction_position = transactions.iter().position(|v| v.to_string() == tx_id.to_string()).unwrap();
    println!("tx_index: {:?}", transaction_position);
    let merkle_proof = merkle_proof_calculator(transactions, transaction_position);
    let merkle_proof_string_list = merkle_proof.iter().map(|v| v.to_string()).collect::<Vec<String>>();
    println!("merkle_proof: {:?}", merkle_proof_string_list);

    let proof_args = ProofArgs {
        tx_id: tx_id.parse().unwrap(),
        tx_block_blockhash: H256(block.header.block_hash().to_byte_array()),
        tx_index: transaction_position as u64,
        merkle_proof,
        confirmations,
    };

    println!("");
    println!("Verify by directly calling the btc-client.testnet interface:");
    println!("near call btc-client.testnet verify_transaction_inclusion {:?} --base64 --accountId $YOUR_NEAR_ACCOUNT", STANDARD.encode(borsh::to_vec(&proof_args).unwrap()));
    
    println!("");
    println!("Verify through cross-contract calls to the btc-client.testnet interface.:");
    println!("near call use-light-client-example.testnet verify_transaction_inclusion '{{\"tx_id\": \"{tx_id}\", \"tx_block_blockhash\":\"{block_hash}\", \"tx_index\":{transaction_position}, \"merkle_proof\":{merkle_proof_string_list:?}, \"confirmations\":{confirmations}}}' --accountId $YOUR_NEAR_ACCOUNT");

    Ok(())
}