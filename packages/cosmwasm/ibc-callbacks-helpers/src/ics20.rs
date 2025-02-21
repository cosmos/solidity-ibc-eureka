//! Helpers for ICS20 Packets

use cosmwasm_std::IbcPacket;
use ibc_proto::ibc::apps::transfer::v2::FungibleTokenPacketData;

/// Extension trait for ICS20 packets
/// Implemented for [`IbcPacket`]
pub trait ICS20PacketExt {
    /// Get the ICS20 packet data for a fungible token transfer
    /// Returns `None` if the packet data cannot be parsed as a fungible token transfer
    fn get_ics20_ftpd(&self) -> Option<FungibleTokenPacketData>;
}

impl ICS20PacketExt for IbcPacket {
    fn get_ics20_ftpd(&self) -> Option<FungibleTokenPacketData> {
        todo!()
    }
}
