pub mod rpc {
    tonic::include_proto!("aggregator");
}

pub mod aggregator;
pub mod config;
pub mod error;

#[cfg(test)]
pub mod mock_attestor;