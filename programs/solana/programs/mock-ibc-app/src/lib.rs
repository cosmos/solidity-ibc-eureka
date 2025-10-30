use anchor_lang::prelude::*;
use solana_ibc_macros::ibc_app;
use solana_ibc_types::{OnAcknowledgementPacketMsg, OnRecvPacketMsg, OnTimeoutPacketMsg};

declare_id!("4Fo5RuY7bEPZNz1FjkM9cUkUVc2BVhdYBjDA8P6Tmox1");

/// Mock IBC Application Program for Testing
///
/// This program is a minimal implementation of the IBC app interface
/// used only for testing the router. It has no state and no logic,
/// just accepts the calls and returns success.
#[ibc_app]
pub mod mock_ibc_app {
    use super::*;
    use anchor_lang::solana_program::program::set_return_data;

    pub fn on_recv_packet(_ctx: Context<OnRecvPacket>, msg: OnRecvPacketMsg) -> Result<()> {
        // Check for special test scenarios based on the packet data
        if let Some(data) = msg.payload.value.get(0..16) {
            if data == b"RETURN_ERROR_ACK" {
                // Return the universal error acknowledgement
                set_return_data(b"error");
                return Ok(());
            }
        }

        // Default: Return the expected acknowledgement for tests
        set_return_data(b"packet received");
        Ok(())
    }

    pub fn on_acknowledgement_packet(
        _ctx: Context<OnAcknowledgementPacket>,
        _msg: OnAcknowledgementPacketMsg,
    ) -> Result<()> {
        Ok(())
    }

    pub fn on_timeout_packet(
        _ctx: Context<OnTimeoutPacket>,
        _msg: OnTimeoutPacketMsg,
    ) -> Result<()> {
        Ok(())
    }
}

#[derive(Accounts)]
pub struct OnRecvPacket<'info> {
    /// CHECK: Mock app doesn't validate or use this account
    pub app_state: AccountInfo<'info>,

    /// CHECK: Mock app doesn't validate the router
    pub router_program: AccountInfo<'info>,
}

#[derive(Accounts)]
pub struct OnAcknowledgementPacket<'info> {
    /// CHECK: Mock app doesn't validate or use this account
    pub app_state: AccountInfo<'info>,

    /// CHECK: Mock app doesn't validate the router
    pub router_program: AccountInfo<'info>,
}

#[derive(Accounts)]
pub struct OnTimeoutPacket<'info> {
    /// CHECK: Mock app doesn't validate or use this account
    pub app_state: AccountInfo<'info>,

    /// CHECK: Mock app doesn't validate the router
    pub router_program: AccountInfo<'info>,
}
