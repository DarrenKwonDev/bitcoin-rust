# bitcoin-rust

Lukáš Hozda의 <Building Bitcoin in Rust> 를 따라 간 repo  


## 서명 알고리즘

```text
ECDSA (타원곡선 디지털 서명 알고리즘)
  └─ 인터페이스/알고리즘 (서명/검증 방식)
      ├─ secp256k1 곡선 (k256 크레이트)
      ├─ NIST P-256 곡선 (p256 크레이트)  
      └─ NIST P-384 곡선 (p384 크레이트)

```

