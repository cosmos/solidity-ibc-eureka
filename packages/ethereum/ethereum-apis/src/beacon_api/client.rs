use ethereum_types::consensus::{
    light_client_header::{LightClientFinalityUpdate, LightClientUpdate},
    spec::Spec,
};
use reqwest::{Client, StatusCode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use super::error::{BeaconApiClientError, InternalServerError, NotFoundError};

pub struct BeaconApiClient {
    pub client: Client,
    pub base_url: String,
}

impl BeaconApiClient {
    pub fn new(base_url: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
        }
    }

    pub async fn spec(&self) -> Result<Response<Spec>, BeaconApiClientError> {
        self.get_json("/eth/v1/config/spec").await
    }

    pub async fn finality_update(
        &self,
    ) -> Result<Response<LightClientFinalityUpdate, Version>, BeaconApiClientError> {
        self.get_json("/eth/v1/beacon/light_client/finality_update")
            .await
    }

    pub async fn light_client_updates(
        &self,
        start_period: u64,
        count: u64,
    ) -> Result<Vec<LightClientUpdate>, BeaconApiClientError> {
        self.get_json(format!(
            "/eth/v1/beacon/light_client/updates?start_period={start_period}&count={count}"
        ))
        .await
    }

    // Helper functions

    async fn get_json<T: DeserializeOwned>(
        &self,
        path: impl Into<String>,
    ) -> Result<T, BeaconApiClientError> {
        let url = format!("{}{}", self.base_url, path.into());

        //debug!(%url, "get_json");

        let res = self.client.get(url).send().await?;

        match res.status() {
            StatusCode::OK => {
                let bytes = res.bytes().await?;

                //trace!(response = %String::from_utf8_lossy(&bytes), "get_json");

                Ok(serde_json::from_slice(&bytes).map_err(BeaconApiClientError::Json)?)
            }
            StatusCode::NOT_FOUND => Err(BeaconApiClientError::NotFound(
                res.json::<NotFoundError>().await?,
            )),
            StatusCode::INTERNAL_SERVER_ERROR => Err(BeaconApiClientError::Internal(
                res.json::<InternalServerError>().await?,
            )),
            code => Err(BeaconApiClientError::Other {
                code,
                text: res.text().await?,
            }),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Response<Data, Extra = Nil> {
    pub data: Data,
    #[serde(flatten)]
    pub extra: Extra,
}

impl<Data, Extra> Response<Data, Extra> {
    pub fn map_data<T>(self, f: impl FnOnce(Data) -> T) -> Response<T, Extra> {
        Response {
            data: f(self.data),
            extra: self.extra,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Nil {}

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct Version {
    pub version: EthConsensusVersion,
}
