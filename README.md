# bitcoin-rust

Lukáš Hozda의 「Building Bitcoin in Rust」 를 따라 간 repo  

## not actual bitcoin

* Double-Spend Transactions: Currently first-come, first-served.
Real Bitcoin prioritizes by higher miner fees, often allowing transaction replacement.

* Mempool Cleanup: UTXO removal from the mempool isn't incremental.
  real nodes handle this more efficiently post-block.

* Mining Template Validation: Uses a 5-second poll to check template validity.
Real Bitcoin relies on push (Stratum) or long-polling for updates.

