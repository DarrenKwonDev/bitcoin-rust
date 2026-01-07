use ecdsa::{signature::Signer, Signature as ECDSASignature, SigningKey, VerifyingKey};
use k256::Secp256k1;
use serde::{Deserialize, Serialize};

// ----------------------------------------------
/// secp256k1을 사용한 서명. private key로 생성
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Signature(ECDSASignature<Secp256k1>);

// ----------------------------------------------
/// secp256k1 곡선의 공개키. 특정 private key로 서명되었는가 signature를 검증
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct PublicKey(VerifyingKey<Secp256k1>);

// ----------------------------------------------
/// secp256k1 곡선의 비밀키  
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PrivateKey(#[serde(with = "signkey_serde")] pub SigningKey<Secp256k1>);

impl PrivateKey {
    pub fn new_key() -> Self {
        PrivateKey(SigningKey::random(&mut rand::thread_rng()))
    }

    pub fn public_key(&self) -> PublicKey {
        PublicKey(self.0.verifying_key().clone())
    }
}

mod signkey_serde {
    use serde::Deserialize;

    pub fn serialize<S>(
        key: &super::SigningKey<super::Secp256k1>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bytes(&key.to_bytes())
    }

    // ---------------------------
    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<super::SigningKey<super::Secp256k1>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes: Vec<u8> = Vec::<u8>::deserialize(deserializer)?;
        Ok(super::SigningKey::from_slice(&bytes).unwrap())
    }
}
