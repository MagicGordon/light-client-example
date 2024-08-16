use std::{fmt, str::FromStr};

use near_sdk::{
    env, ext_contract, log, near, promise_result_as_success, serde::{
        self,
        de::{self, Visitor},
        Deserialize, Serialize,
    }, serde_json, Promise
};

#[near(serializers = [borsh])]
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

#[near(serializers = [borsh])]
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

#[ext_contract(ext_btc_light_client)]
pub trait BtcLightClient {
    fn verify_transaction_inclusion(&self, #[serializer(borsh)] args: ProofArgs) -> bool;
}

#[derive(Default)]
#[near(contract_state)]
pub struct Contract {}

#[near]
impl Contract {
    pub fn verify_transaction_inclusion(
        &self,
        tx_id: String,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
        confirmations: u64,
    ) -> Promise {
        ext_btc_light_client::ext("btc-client.testnet".parse().unwrap())
            .verify_transaction_inclusion(ProofArgs {
                tx_id: tx_id.parse().expect("Invalid tx_id"),
                tx_block_blockhash: tx_block_blockhash
                    .parse()
                    .expect("Invalid tx_block_blockhash"),
                tx_index,
                merkle_proof: merkle_proof
                    .into_iter()
                    .map(|v| {
                        v.parse()
                            .expect(format!("Invalid merkle_proof: {:?}", v).as_str())
                    })
                    .collect(),
                confirmations,
            })
            .then(
                Self::ext(env::current_account_id())
                    .internal_verify_withdraw_callback(),
            )
    }

    #[private]
    pub fn internal_verify_withdraw_callback(&mut self) -> bool {
        if let Some(result_bytes) = promise_result_as_success() {
            let result = serde_json::from_slice::<bool>(&result_bytes).unwrap();
            if result {
                log!("business logic");
            }
            result
        } else {
            false
        }
    }
}
