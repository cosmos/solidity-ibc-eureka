//! Helpers for ICS20 Packets

use alloy_sol_types::SolType;
use cosmwasm_std::IbcPacket;
use ibc_eureka_solidity_types::msgs::IICS20TransferMsgs::FungibleTokenPacketData as AbiFungibleTokenPacketData;
use ibc_proto_eureka::ibc::apps::transfer::v2::FungibleTokenPacketData;
use prost::Message;
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
        let data = self.data.as_slice();

        // Try to parse the packet data as a JSON encoded FungibleTokenPacketData
        if let Ok(ftpd) = serde_json::from_slice(data) {
            return Some(ftpd);
        }

        // Try to parse the packet data as an ABI encoded FungibleTokenPacketData
        if let Ok(ftpd) = AbiFungibleTokenPacketData::abi_decode(data, true) {
            return Some(ftpd.into());
        }

        // Try to parse the packet data as a protobuf encoded FungibleTokenPacketData
        if let Ok(ftpd) = FungibleTokenPacketData::decode(data) {
            return Some(ftpd);
        }

        None
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
