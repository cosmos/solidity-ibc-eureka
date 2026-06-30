//! Verifies the JSON emitted by the Go config builder deserializes into the
//! attested transaction-builder mode.
use proof_api_cosmos_to_cosmos::{CosmosToCosmosConfig, TxBuilderMode};

#[test]
fn attested_config_deserializes_to_attested_mode() {
    let json = serde_json::json!({
        "src_rpc_url": "http://a:26657",
        "target_rpc_url": "http://b:26657",
        "signer_address": "cosmos1xyz",
        "mode": { "attested": {
            "attestor": {
                "attestor_query_timeout_ms": 5000,
                "quorum_threshold": 1,
                "attestor_endpoints": ["http://127.0.0.1:2025"]
            },
            "cache": { "state_cache_max_entries": 100000, "packet_cache_max_entries": 100000 }
        }}
    });
    let cfg: CosmosToCosmosConfig = serde_json::from_value(json).unwrap();
    assert!(
        matches!(cfg.mode, TxBuilderMode::Attested(_)),
        "expected Attested, got {:?}",
        cfg.mode
    );
}

#[test]
fn missing_mode_defaults_to_native() {
    let json = serde_json::json!({
        "src_rpc_url": "http://a:26657",
        "target_rpc_url": "http://b:26657",
        "signer_address": "cosmos1xyz"
    });
    let cfg: CosmosToCosmosConfig = serde_json::from_value(json).unwrap();
    assert!(matches!(cfg.mode, TxBuilderMode::Native));
}
