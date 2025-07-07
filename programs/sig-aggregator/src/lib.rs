pub mod rpc {
    tonic::include_proto!("aggregator");

    pub(crate) const FILE_DESCRIPTOR_SET: &[u8] =
        tonic::include_file_descriptor_set!("aggregator_descriptor");
}

pub mod aggregator;
pub mod config;
pub mod error;
pub mod cli;
pub mod attestor_data;
pub mod server;

#[cfg(test)]
pub mod mock_attestor;
