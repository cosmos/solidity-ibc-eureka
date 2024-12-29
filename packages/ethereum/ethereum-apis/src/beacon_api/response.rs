//! This module defines the response types for the Beacon API.

use serde::{Deserialize, Serialize};

/// The response structure returned by the Beacon API.
#[derive(Debug, Serialize, Deserialize)]
pub struct Response<Data, Extra = EmptyExtra> {
    /// The main data of the response.
    pub data: Data,
    /// Extra data of the response.
    #[serde(flatten)]
    pub extra: Extra,
}

//impl<Data, Extra> Response<Data, Extra> {
//    pub fn map_data<T>(self, f: impl FnOnce(Data) -> T) -> Response<T, Extra> {
//        Response {
//            data: f(self.data),
//            extra: self.extra,
//        }
//    }
//}

/// The default empty extra data for `Response`.
#[derive(Debug, Serialize, Deserialize)]
pub struct EmptyExtra {}

/// The version of the Ethereum consensus.
#[derive(Debug, Serialize, Deserialize)]
#[allow(missing_docs)]
pub enum EthConsensusVersion {
    #[serde(rename = "phase0")]
    Phase0,
    #[serde(rename = "altair")]
    Altair,
    #[serde(rename = "bellatrix")]
    Bellatrix,
    #[serde(rename = "capella")]
    Capella,
    #[serde(rename = "deneb")]
    Deneb,
}

/// The version response structure returned by the Beacon API.
#[derive(Debug, Serialize, Deserialize)]
pub struct Version {
    /// The version of the Ethereum consensus.
    pub version: EthConsensusVersion,
}
