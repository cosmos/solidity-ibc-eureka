//! Solidity types for ICS26Router.sol

#[cfg(feature = "rpc")]
alloy_sol_types::sol!(
    #[sol(rpc)]
    #[derive(Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
    #[allow(missing_docs, clippy::pedantic, warnings)]
    router,
    "../../abi/ICS26Router.json"
);

// NOTE: Some environments won't compile with the `rpc` features.
#[cfg(not(feature = "rpc"))]
alloy_sol_types::sol!(
    #[derive(Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
    #[allow(missing_docs, clippy::pedantic)]
    router,
    "../../abi/ICS26Router.json"
);

impl IICS26RouterMsgs::Packet {
    /// Returns the commitment path for the packet.
    #[must_use]
    pub fn commitment_path(&self) -> Vec<u8> {
        let mut path = Vec::new();
        path.extend_from_slice(self.sourceChannel.as_bytes());
        path.push(1_u8);
        path.extend_from_slice(&u64::from(self.sequence).to_be_bytes());
        path
    }

    /// Returns the commitment path for the receipt.
    #[must_use]
    pub fn receipt_commitment_path(&self) -> Vec<u8> {
        let mut path = Vec::new();
        path.extend_from_slice(self.destChannel.as_bytes());
        path.push(2_u8);
        path.extend_from_slice(&u64::from(self.sequence).to_be_bytes());
        path
    }

    /// Returns the commitment path for the acknowledgement.
    #[must_use]
    pub fn ack_commitment_path(&self) -> Vec<u8> {
        let mut path = Vec::new();
        path.extend_from_slice(self.destChannel.as_bytes());
        path.push(3_u8);
        path.extend_from_slice(&u64::from(self.sequence).to_be_bytes());
        path
    }
}
