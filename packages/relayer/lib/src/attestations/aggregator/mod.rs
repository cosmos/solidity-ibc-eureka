pub mod rpc {
    tonic::include_proto!("aggregator");
    tonic::include_proto!("ibc_attestor");
}

pub mod aggregator;
pub mod attestor_data;
pub mod config;
