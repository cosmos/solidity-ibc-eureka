pub mod rpc {
    tonic::include_proto!("aggregator");
}

pub mod attestor;
pub mod aggregator;
pub mod config;
pub mod error;
