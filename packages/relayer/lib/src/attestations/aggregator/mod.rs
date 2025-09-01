pub mod rpc {
    tonic::include_proto!("aggregator");
    tonic::include_proto!("ibc_attestor");

    pub(crate) const AGG_FILE_DESCRIPTOR: &[u8] =
        tonic::include_file_descriptor_set!("aggregator_descriptor");
}

pub mod aggregator;
pub mod attestor_data;
pub mod config;
pub mod server;
