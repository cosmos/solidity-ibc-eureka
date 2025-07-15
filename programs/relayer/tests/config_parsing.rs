use ibc_eureka_relayer_core::config::{parse_config, RelayerConfig};
use ibc_eureka_relayer_cosmos_to_cosmos::CosmosToCosmosConfig;
use ibc_eureka_relayer_cosmos_to_eth::CosmosToEthConfig;
use ibc_eureka_relayer_eth_to_cosmos::EthToCosmosConfig;
use serde_json::json;

/// Build a minimal valid relayer JSON configuration containing a single
/// `cosmos_to_eth` module so that we can tweak it in each test.
fn base_relayer_json() -> serde_json::Value {
    json!({
        "server": {
            "address": "127.0.0.1",
            "port": 3000,
            "log_level": "info"
        },
        "modules": [
            {
                "name": "cosmos_to_eth",
                "src_chain": "cosmoshub-4",
                "dst_chain": "eth-1",
                "enabled": true,
                "config": {
                    "tm_rpc_url": "http://localhost:26657",
                    "ics26_address": "0x0000000000000000000000000000000000000000",
                    "eth_rpc_url": "http://localhost:8545",
                    "sp1_prover": { "type": "mock" },
                    "sp1_programs": {
                        "update_client": "./uc",
                        "membership": "./mem",
                        "update_client_and_membership": "./ucm",
                        "misbehaviour": "./mis"
                    }
                }
            }
        ]
    })
}

/// Build a minimal valid relayer JSON configuration containing a single
/// `eth_to_cosmos` module.
fn base_eth_to_cosmos_json() -> serde_json::Value {
    json!({
        "server": {
            "address": "127.0.0.1",
            "port": 3000,
            "log_level": "info"
        },
        "modules": [
            {
                "name": "eth_to_cosmos",
                "src_chain": "eth-1",
                "dst_chain": "cosmoshub-4",
                "enabled": true,
                "config": {
                    "ics26_address": "0x0000000000000000000000000000000000000000",
                    "tm_rpc_url": "http://localhost:26657",
                    "eth_rpc_url": "http://localhost:8545",
                    "eth_beacon_api_url": "http://localhost:5052",
                    "signer_address": "cosmos1abc",
                    "mock": true
                }
            }
        ]
    })
}

/// Build a minimal valid relayer JSON configuration containing a single
/// `cosmos_to_cosmos` module.
fn base_cosmos_to_cosmos_json() -> serde_json::Value {
    json!({
        "server": {
            "address": "127.0.0.1",
            "port": 3000,
            "log_level": "info"
        },
        "modules": [
            {
                "name": "cosmos_to_cosmos",
                "src_chain": "cosmoshub-4",
                "dst_chain": "osmosis-1",
                "enabled": true,
                "config": {
                    "src_rpc_url": "http://localhost:26657",
                    "target_rpc_url": "http://localhost:36657",
                    "signer_address": "cosmos1abc"
                }
            }
        ]
    })
}

// ----------------- Top-level RelayerConfig deserialization -----------------

#[test]
fn top_level_missing_server_field_fails() {
    let mut json_val = base_relayer_json();
    // Remove the `server` object entirely
    json_val.as_object_mut().unwrap().remove("server");
    let err = serde_json::from_value::<RelayerConfig>(json_val).unwrap_err();
    assert!(err.to_string().contains("server"));
}

#[test]
fn top_level_port_wrong_type_fails() {
    let mut json_val = base_relayer_json();
    json_val["server"]["port"] = json!("not_a_number");
    let err = serde_json::from_value::<RelayerConfig>(json_val).unwrap_err();
    assert!(err.to_string().contains("invalid type"));
}

#[test]
fn top_level_missing_modules_field_fails() {
    let mut json_val = base_relayer_json();
    json_val.as_object_mut().unwrap().remove("modules");
    let err = serde_json::from_value::<RelayerConfig>(json_val).unwrap_err();
    assert!(err.to_string().contains("modules"));
}

#[test]
fn top_level_modules_wrong_type_fails() {
    let mut json_val = base_relayer_json();
    json_val["modules"] = json!("not_an_array");
    let err = serde_json::from_value::<RelayerConfig>(json_val).unwrap_err();
    assert!(err.to_string().contains("invalid type"));
}

// ----------------- Module level -----------------

#[test]
fn module_level_missing_name_field_fails() {
    let mut json_val = base_relayer_json();
    json_val["modules"][0]
        .as_object_mut()
        .unwrap()
        .remove("name");
    let err = serde_json::from_value::<RelayerConfig>(json_val).unwrap_err();
    assert!(err.to_string().contains("name"));
}

#[test]
fn module_level_missing_src_chain_field_fails() {
    let mut json_val = base_relayer_json();
    json_val["modules"][0]
        .as_object_mut()
        .unwrap()
        .remove("src_chain");
    let err = serde_json::from_value::<RelayerConfig>(json_val).unwrap_err();
    assert!(err.to_string().contains("src_chain"));
}

#[test]
fn module_level_missing_dst_chain_field_fails() {
    let mut json_val = base_relayer_json();
    json_val["modules"][0]
        .as_object_mut()
        .unwrap()
        .remove("dst_chain");
    let err = serde_json::from_value::<RelayerConfig>(json_val).unwrap_err();
    assert!(err.to_string().contains("dst_chain"));
}

#[test]
fn module_level_missing_config_field_fails() {
    let mut json_val = base_relayer_json();
    json_val["modules"][0]
        .as_object_mut()
        .unwrap()
        .remove("config");
    let err = serde_json::from_value::<RelayerConfig>(json_val).unwrap_err();
    assert!(err.to_string().contains("config"));
}

// ----------------- Cosmos to Cosmos -----------------

#[test]
fn cosmos_to_cosmos_missing_target_rpc_url() {
    let mut json_val = base_cosmos_to_cosmos_json();
    json_val["modules"][0]["config"]
        .as_object_mut()
        .unwrap()
        .remove("target_rpc_url");
    let relayer_cfg: RelayerConfig = serde_json::from_value(json_val).unwrap();
    let module_cfg = &relayer_cfg.modules[0].config;
    let err = parse_config::<CosmosToCosmosConfig>(module_cfg.clone()).unwrap_err();
    assert!(err.to_string().contains("target_rpc_url"));
}

#[test]
fn cosmos_to_cosmos_full_config_parses_successfully() -> anyhow::Result<()> {
    let json_val = base_cosmos_to_cosmos_json();
    let relayer_cfg: RelayerConfig = serde_json::from_value(json_val.clone())?;
    assert_eq!(relayer_cfg.modules.len(), 1);
    let module_cfg = &relayer_cfg.modules[0].config;
    let _parsed: CosmosToCosmosConfig = parse_config(module_cfg.clone())?;
    Ok(())
}

#[test]
fn cosmos_to_cosmos_missing_required_field_yields_path_error() {
    let mut json_val = base_cosmos_to_cosmos_json();
    // Remove `src_rpc_url` field
    if let Some(obj) = json_val["modules"][0]["config"].as_object_mut() {
        obj.remove("src_rpc_url");
    }
    let relayer_cfg: RelayerConfig = serde_json::from_value(json_val).unwrap();
    let module_cfg = &relayer_cfg.modules[0].config;
    let err = parse_config::<CosmosToCosmosConfig>(module_cfg.clone()).unwrap_err();
    assert!(err.to_string().contains("src_rpc_url"));
}

#[test]
fn cosmos_to_cosmos_invalid_signer_address_type() {
    let mut json_val = base_cosmos_to_cosmos_json();
    // Set `signer_address` to number
    json_val["modules"][0]["config"]["signer_address"] = json!(42);
    let relayer_cfg: RelayerConfig = serde_json::from_value(json_val).unwrap();
    let module_cfg = &relayer_cfg.modules[0].config;
    let err = parse_config::<CosmosToCosmosConfig>(module_cfg.clone()).unwrap_err();
    assert!(err.to_string().contains("signer_address"));
}

// ----------------- Cosmos to Eth -----------------

#[test]
fn cosmos_to_eth_missing_misbehaviour_path() {
    let mut json_val = base_relayer_json();
    // Remove misbehaviour path
    json_val["modules"][0]["config"]["sp1_programs"]
        .as_object_mut()
        .unwrap()
        .remove("misbehaviour");
    let relayer_cfg: RelayerConfig = serde_json::from_value(json_val).unwrap();
    let module_cfg = &relayer_cfg.modules[0].config;
    let err = parse_config::<CosmosToEthConfig>(module_cfg.clone()).unwrap_err();
    assert!(err.to_string().contains("misbehaviour"));
}

#[test]
fn full_config_parses_successfully() -> anyhow::Result<()> {
    let json_val = base_relayer_json();
    let relayer_cfg: RelayerConfig = serde_json::from_value(json_val.clone())?;

    // The top-level structure should deserialize fine.
    assert_eq!(relayer_cfg.modules.len(), 1);

    // Now invoke the path-aware parse helper on the module config.
    let module_cfg = &relayer_cfg.modules[0].config;
    let _parsed: CosmosToEthConfig = parse_config(module_cfg.clone())?;
    Ok(())
}

#[test]
fn missing_required_field_yields_path_error() {
    let mut json_val = base_relayer_json();
    // Remove `tm_rpc_url` from the module config.
    if let Some(tm) = json_val["modules"][0]["config"].as_object_mut() {
        tm.remove("tm_rpc_url");
    }

    let relayer_cfg: RelayerConfig = serde_json::from_value(json_val).unwrap();
    let module_cfg = &relayer_cfg.modules[0].config;
    let err = parse_config::<CosmosToEthConfig>(module_cfg.clone()).unwrap_err();
    assert!(err.to_string().contains("tm_rpc_url"));
}

#[test]
fn invalid_enum_variant_yields_path_error() {
    let mut json_val = base_relayer_json();
    // Set an invalid variant for `sp1_prover.type`.
    json_val["modules"][0]["config"]["sp1_prover"]["type"] = json!("invalid");

    let relayer_cfg: RelayerConfig = serde_json::from_value(json_val).unwrap();
    let module_cfg = &relayer_cfg.modules[0].config;
    let err = parse_config::<CosmosToEthConfig>(module_cfg.clone()).unwrap_err();
    assert!(err.to_string().contains("config error at sp1_prover.type"));
}

#[test]
fn cosmos_to_eth_invalid_sp1_program_path_type() {
    let mut json_val = base_relayer_json();
    // Set `sp1_programs.update_client` to a number instead of string
    json_val["modules"][0]["config"]["sp1_programs"]["update_client"] = json!(123);
    let relayer_cfg: RelayerConfig = serde_json::from_value(json_val).unwrap();
    let module_cfg = &relayer_cfg.modules[0].config;
    let err = parse_config::<CosmosToEthConfig>(module_cfg.clone()).unwrap_err();
    assert!(err.to_string().contains("sp1_programs.update_client"));
}

// ----------------- Eth to Cosmos -----------------

#[test]
fn eth_to_cosmos_missing_tm_rpc_url() {
    let mut json_val = base_eth_to_cosmos_json();
    json_val["modules"][0]["config"]
        .as_object_mut()
        .unwrap()
        .remove("tm_rpc_url");
    let relayer_cfg: RelayerConfig = serde_json::from_value(json_val).unwrap();
    let module_cfg = &relayer_cfg.modules[0].config;
    let err = parse_config::<EthToCosmosConfig>(module_cfg.clone()).unwrap_err();
    assert!(err.to_string().contains("tm_rpc_url"));
}

#[test]
fn eth_to_cosmos_full_config_parses_successfully() -> anyhow::Result<()> {
    let json_val = base_eth_to_cosmos_json();
    let relayer_cfg: RelayerConfig = serde_json::from_value(json_val.clone())?;
    assert_eq!(relayer_cfg.modules.len(), 1);
    let module_cfg = &relayer_cfg.modules[0].config;
    let _parsed: EthToCosmosConfig = parse_config(module_cfg.clone())?;
    Ok(())
}

#[test]
fn eth_to_cosmos_missing_required_field_yields_path_error() {
    let mut json_val = base_eth_to_cosmos_json();
    // Remove `eth_rpc_url` field
    if let Some(obj) = json_val["modules"][0]["config"].as_object_mut() {
        obj.remove("eth_rpc_url");
    }
    let relayer_cfg: RelayerConfig = serde_json::from_value(json_val).unwrap();
    let module_cfg = &relayer_cfg.modules[0].config;
    let err = parse_config::<EthToCosmosConfig>(module_cfg.clone()).unwrap_err();
    assert!(err.to_string().contains("eth_rpc_url"));
}

#[test]
fn eth_to_cosmos_invalid_mock_type() {
    let mut json_val = base_eth_to_cosmos_json();
    // Set `mock` to a string instead of bool
    json_val["modules"][0]["config"]["mock"] = json!("yes");
    let relayer_cfg: RelayerConfig = serde_json::from_value(json_val).unwrap();
    let module_cfg = &relayer_cfg.modules[0].config;
    let err = parse_config::<EthToCosmosConfig>(module_cfg.clone()).unwrap_err();
    assert!(err.to_string().contains("mock"));
}
