//! Helpers for ICS20 Packets

use cosmwasm_std::IbcPacket;
use ibc_proto::ibc::apps::transfer::v2::FungibleTokenPacketData;
use sha2::{Digest, Sha256};

/// Extension trait for ICS20 packets
/// Implemented for [`IbcPacket`]
pub trait ICS20PacketExt {
    /// Get the ICS20 packet data for a fungible token transfer
    /// Returns `None` if the packet data cannot be parsed as a fungible token transfer
    fn get_ics20_ftpd(&self) -> Option<FungibleTokenPacketData>;

    /// Get the `CosmosSDK` denom of the token being received
    /// Returns `None` if the packet data cannot be parsed as a fungible token transfer
    fn get_recv_denom(&self) -> Option<String>;
}

impl ICS20PacketExt for IbcPacket {
    fn get_ics20_ftpd(&self) -> Option<FungibleTokenPacketData> {
        serde_json::from_slice(self.data.as_slice()).ok()
    }

    fn get_recv_denom(&self) -> Option<String> {
        let packet_denom = self.get_ics20_ftpd()?.denom;
        let prefix = format!("{}/{}", self.src.port_id, self.src.channel_id);

        let is_returning_to_origin = packet_denom.starts_with(&prefix);
        let ibc_denom = if is_returning_to_origin {
            packet_denom.trim_start_matches(&prefix).to_string()
        } else {
            format!(
                "{}/{}/{packet_denom}",
                self.dest.port_id, self.dest.channel_id
            )
        };

        let denom_hash = Sha256::digest(ibc_denom.as_bytes()).to_vec();
        Some(format!("ibc/{}", hex::encode(denom_hash)))
    }
}
