use crate::constants::IBC_CPI_INSTRUCTION_CAPACITY;
use crate::errors::RouterError;
use crate::state::Packet;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
use anchor_lang::solana_program::program::{get_return_data, invoke};
use solana_ibc_types::{
    ibc_app_instructions, OnAcknowledgementPacketMsg, OnRecvPacketMsg, OnTimeoutPacketMsg, Payload,
};

/// Common accounts required for IBC app CPI calls
#[derive(Clone)]
pub struct IbcAppCpiAccounts<'a> {
    pub ibc_app_program: AccountInfo<'a>,
    pub app_state: AccountInfo<'a>,
    pub router_program: AccountInfo<'a>,
    pub payer: AccountInfo<'a>,
    pub system_program: AccountInfo<'a>,
}

/// IBC app CPI calls
pub struct IbcAppCpi<'a> {
    accounts: IbcAppCpiAccounts<'a>,
}

impl<'a> IbcAppCpi<'a> {
    pub const fn new(accounts: IbcAppCpiAccounts<'a>) -> Self {
        Self { accounts }
    }

    pub fn on_recv_packet(
        &self,
        packet: &Packet,
        payload: &Payload,
        relayer: &Pubkey,
        remaining_accounts: &[AccountInfo<'a>],
    ) -> Result<Vec<u8>> {
        let msg = OnRecvPacketMsg {
            source_client: packet.source_client.clone(),
            dest_client: packet.dest_client.clone(),
            sequence: packet.sequence,
            payload: payload.clone(),
            relayer: *relayer,
        };

        self.invoke_with_discriminator(
            ibc_app_instructions::on_recv_packet_discriminator(),
            msg,
            remaining_accounts,
        )?;

        self.get_app_acknowledgement()
    }

    pub fn on_acknowledgement_packet(
        &self,
        packet: &Packet,
        payload: &Payload,
        acknowledgement: &[u8],
        relayer: &Pubkey,
        remaining_accounts: &[AccountInfo<'a>],
    ) -> Result<()> {
        let msg = OnAcknowledgementPacketMsg {
            source_client: packet.source_client.clone(),
            dest_client: packet.dest_client.clone(),
            sequence: packet.sequence,
            payload: payload.clone(),
            acknowledgement: acknowledgement.to_vec(),
            relayer: *relayer,
        };

        self.invoke_with_discriminator(
            ibc_app_instructions::on_acknowledgement_packet_discriminator(),
            msg,
            remaining_accounts,
        )
    }

    pub fn on_timeout_packet(
        &self,
        packet: &Packet,
        payload: &Payload,
        relayer: &Pubkey,
        remaining_accounts: &[AccountInfo<'a>],
    ) -> Result<()> {
        let msg = OnTimeoutPacketMsg {
            source_client: packet.source_client.clone(),
            dest_client: packet.dest_client.clone(),
            sequence: packet.sequence,
            payload: payload.clone(),
            relayer: *relayer,
        };

        self.invoke_with_discriminator(
            ibc_app_instructions::on_timeout_packet_discriminator(),
            msg,
            remaining_accounts,
        )
    }

    fn invoke_with_discriminator<T: AnchorSerialize>(
        &self,
        discriminator: [u8; 8],
        msg: T,
        remaining_accounts: &[AccountInfo<'a>],
    ) -> Result<()> {
        let mut instruction_data = Vec::with_capacity(IBC_CPI_INSTRUCTION_CAPACITY);
        instruction_data.extend_from_slice(&discriminator);
        msg.serialize(&mut instruction_data)?;

        let account_metas = self.build_account_metas(remaining_accounts);

        let instruction = Instruction::new_with_bytes(
            *self.accounts.ibc_app_program.key,
            &instruction_data,
            account_metas,
        );

        let account_infos = self.build_account_infos(remaining_accounts);

        invoke(&instruction, &account_infos)?;

        Ok(())
    }

    fn build_account_metas(&self, remaining_accounts: &[AccountInfo<'a>]) -> Vec<AccountMeta> {
        let mut metas = vec![
            AccountMeta::new(*self.accounts.app_state.key, false),
            AccountMeta::new_readonly(*self.accounts.router_program.key, false),
            AccountMeta::new(*self.accounts.payer.key, true),
            AccountMeta::new_readonly(*self.accounts.system_program.key, false),
        ];

        metas.extend(remaining_accounts.iter().map(|account| AccountMeta {
            pubkey: *account.key,
            is_signer: account.is_signer,
            is_writable: account.is_writable,
        }));

        metas
    }

    fn build_account_infos(&self, remaining_accounts: &[AccountInfo<'a>]) -> Vec<AccountInfo<'a>> {
        let mut infos = vec![
            self.accounts.app_state.clone(),
            self.accounts.router_program.clone(),
            self.accounts.payer.clone(),
            self.accounts.system_program.clone(),
        ];
        infos.extend_from_slice(remaining_accounts);
        infos
    }

    fn get_app_acknowledgement(&self) -> Result<Vec<u8>> {
        match get_return_data() {
            Some((program_id, data)) if program_id == *self.accounts.ibc_app_program.key => {
                Ok(data)
            }
            _ => Err(RouterError::InvalidAppResponse.into()),
        }
    }
}
