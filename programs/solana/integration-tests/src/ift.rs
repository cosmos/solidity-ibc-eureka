//! Inter-chain Fungible Token (IFT) transfer instruction builders.
//!
//! Covers SPL / Token-2022 token creation, bridge registration, transfers,
//! finalization, admin operations (pause, admin transfer, admin mint),
//! ABI-encoded GMP ack/timeout packets and on-chain state readers.

use crate::chain::{derive_mock_lc_pdas, Chain};
use crate::gmp::{GMP_PORT_ID, ICS27_VERSION};
use anchor_lang::InstructionData;
use anchor_spl::associated_token::get_associated_token_address_with_program_id;
use ics26_router::state::*;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    system_program,
};

/// ABI content-type used for IFT payload encoding towards EVM chains.
pub const ICS27_ENCODING_ABI: &str = "application/x-solidity-abi";
/// Dummy EVM address for the counterparty IFT contract in tests.
pub const COUNTERPARTY_IFT_ADDRESS: &str = "0x1234567890abcdef1234567890abcdef12345678";
/// Dummy EVM receiver address used in IFT transfer tests.
pub const EVM_RECEIVER: &str = "0xabcdef1234567890abcdef1234567890abcdef12";

// ── Token program selection ─────────────────────────────────────────────

/// Selects between SPL Token and Token-2022 programs.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Spl,
    Token2022,
}

impl TokenKind {
    /// Return the on-chain program ID for this token standard.
    pub const fn program_id(self) -> Pubkey {
        match self {
            Self::Spl => anchor_spl::token::ID,
            Self::Token2022 => anchor_spl::token_2022::ID,
        }
    }

    /// Derive the associated token account for `owner` and `mint`.
    pub fn get_ata(self, owner: &Pubkey, mint: &Pubkey) -> Pubkey {
        get_associated_token_address_with_program_id(owner, mint, &self.program_id())
    }

    /// Read the token balance from an on-chain token account.
    pub async fn read_balance(self, chain: &Chain, token_account: Pubkey) -> u64 {
        use anchor_spl::token_interface::spl_token_2022::{
            extension::StateWithExtensions, state::Account as Token2022Account,
        };
        use solana_sdk::program_pack::Pack;
        let account = chain
            .get_account(token_account)
            .await
            .expect("token account should exist");
        match self {
            Self::Spl => {
                anchor_spl::token::spl_token::state::Account::unpack(&account.data)
                    .expect("valid SPL token account")
                    .amount
            }
            Self::Token2022 => {
                StateWithExtensions::<Token2022Account>::unpack(&account.data)
                    .expect("valid Token 2022 account")
                    .base
                    .amount
            }
        }
    }
}

// ── PDA derivation ──────────────────────────────────────────────────────

/// Derive the IFT `IFTAppState` PDA.
pub fn derive_app_state_pda() -> Pubkey {
    Pubkey::find_program_address(&[ift::constants::IFT_APP_STATE_SEED], &ift::ID).0
}

/// Derive the IFT per-mint state PDA.
pub fn derive_app_mint_state_pda(mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[ift::constants::IFT_APP_MINT_STATE_SEED, mint.as_ref()],
        &ift::ID,
    )
    .0
}

/// Derive the IFT bridge PDA for a given `mint` and `client_id`.
pub fn derive_bridge_pda(mint: &Pubkey, client_id: &str) -> Pubkey {
    Pubkey::find_program_address(
        &[
            ift::constants::IFT_BRIDGE_SEED,
            mint.as_ref(),
            client_id.as_bytes(),
        ],
        &ift::ID,
    )
    .0
}

/// Derive the pending-transfer PDA for a given `mint`, `client_id` and `sequence`.
pub fn derive_pending_transfer_pda(mint: &Pubkey, client_id: &str, sequence: u64) -> Pubkey {
    Pubkey::find_program_address(
        &[
            ift::constants::PENDING_TRANSFER_SEED,
            mint.as_ref(),
            client_id.as_bytes(),
            &sequence.to_le_bytes(),
        ],
        &ift::ID,
    )
    .0
}

/// Derive the IFT mint-authority PDA for a given `mint`.
pub fn derive_mint_authority_pda(mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[ift::constants::MINT_AUTHORITY_SEED, mint.as_ref()],
        &ift::ID,
    )
    .0
}

// ── Instruction builders ────────────────────────────────────────────────

/// Build a `create_and_initialize_spl_token` instruction for SPL Token.
pub fn build_create_spl_token_ix(
    authority: Pubkey,
    payer: Pubkey,
    mint: Pubkey,
    decimals: u8,
) -> Instruction {
    build_create_token_ix(
        authority,
        payer,
        mint,
        anchor_spl::token::ID,
        ift::state::CreateTokenParams::SplToken { decimals },
    )
}

/// Build a `create_and_initialize_spl_token` instruction for Token-2022.
pub fn build_create_token_2022_ix(
    authority: Pubkey,
    payer: Pubkey,
    mint: Pubkey,
    decimals: u8,
    name: String,
    symbol: String,
    uri: String,
) -> Instruction {
    build_create_token_ix(
        authority,
        payer,
        mint,
        anchor_spl::token_2022::ID,
        ift::state::CreateTokenParams::Token2022 {
            decimals,
            name,
            symbol,
            uri,
        },
    )
}

fn build_create_token_ix(
    authority: Pubkey,
    payer: Pubkey,
    mint: Pubkey,
    token_program: Pubkey,
    params: ift::state::CreateTokenParams,
) -> Instruction {
    let app_state_pda = derive_app_state_pda();
    let app_mint_state_pda = derive_app_mint_state_pda(&mint);
    let mint_authority_pda = derive_mint_authority_pda(&mint);

    Instruction {
        program_id: ift::ID,
        accounts: vec![
            AccountMeta::new_readonly(app_state_pda, false),
            AccountMeta::new(app_mint_state_pda, false),
            AccountMeta::new(mint, true),
            AccountMeta::new_readonly(mint_authority_pda, false),
            AccountMeta::new_readonly(authority, true),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(token_program, false),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
        ],
        data: ift::instruction::CreateAndInitializeSplToken { params }.data(),
    }
}

/// Build a `register_ift_bridge` instruction linking a mint to a counterparty.
pub fn build_register_bridge_ix(
    authority: Pubkey,
    payer: Pubkey,
    mint: Pubkey,
    client_id: &str,
    counterparty_ift_address: &str,
) -> Instruction {
    let app_state_pda = derive_app_state_pda();
    let app_mint_state_pda = derive_app_mint_state_pda(&mint);
    let bridge_pda = derive_bridge_pda(&mint, client_id);

    Instruction {
        program_id: ift::ID,
        accounts: vec![
            AccountMeta::new_readonly(app_state_pda, false),
            AccountMeta::new_readonly(app_mint_state_pda, false),
            AccountMeta::new(bridge_pda, false),
            AccountMeta::new_readonly(authority, true),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
        ],
        data: ift::instruction::RegisterIftBridge {
            msg: ift::state::RegisterIFTBridgeMsg {
                client_id: client_id.to_string(),
                counterparty_ift_address: counterparty_ift_address.to_string(),
                chain_options: ift::state::ChainOptions::Evm,
            },
        }
        .data(),
    }
}

/// Build an `admin_mint` instruction to mint tokens to a receiver.
pub fn build_admin_mint_ix(
    authority: Pubkey,
    payer: Pubkey,
    mint: Pubkey,
    receiver: Pubkey,
    amount: u64,
    token_kind: TokenKind,
) -> Instruction {
    let app_state_pda = derive_app_state_pda();
    let app_mint_state_pda = derive_app_mint_state_pda(&mint);
    let mint_authority_pda = derive_mint_authority_pda(&mint);
    let receiver_ata = token_kind.get_ata(&receiver, &mint);

    Instruction {
        program_id: ift::ID,
        accounts: vec![
            AccountMeta::new_readonly(app_state_pda, false),
            AccountMeta::new(app_mint_state_pda, false),
            AccountMeta::new(mint, false),
            AccountMeta::new_readonly(mint_authority_pda, false),
            AccountMeta::new(receiver_ata, false),
            AccountMeta::new_readonly(receiver, false),
            AccountMeta::new_readonly(authority, true),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(token_kind.program_id(), false),
            AccountMeta::new_readonly(anchor_spl::associated_token::ID, false),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
        ],
        data: ift::instruction::AdminMint {
            msg: ift::state::AdminMintMsg { receiver, amount },
        }
        .data(),
    }
}

// ── Transfer instruction ────────────────────────────────────────────────

/// Parameters for building an `ift_transfer` instruction.
pub struct IftTransferParams {
    /// Packet sequence number.
    pub sequence: u64,
    /// Receiver address on the destination chain.
    pub receiver: String,
    /// Amount of tokens to transfer.
    pub amount: u64,
    /// Absolute timeout timestamp (seconds).
    pub timeout_timestamp: u64,
}

/// Output of [`build_ift_transfer_ix`].
#[derive(Debug)]
pub struct IftTransferResult {
    /// The `ift_transfer` instruction to submit.
    pub ix: Instruction,
    /// PDA where the packet commitment is stored.
    pub commitment_pda: Pubkey,
    /// PDA tracking the pending transfer state.
    pub pending_transfer_pda: Pubkey,
}

/// Build an `ift_transfer` instruction that initiates a cross-chain token transfer.
pub fn build_ift_transfer_ix(
    sender: Pubkey,
    payer: Pubkey,
    client_id: &str,
    mint: Pubkey,
    token_kind: TokenKind,
    params: IftTransferParams,
) -> IftTransferResult {
    let (mock_client_state, mock_consensus_state) = derive_mock_lc_pdas(client_id);
    let app_state_pda = derive_app_state_pda();
    let app_mint_state_pda = derive_app_mint_state_pda(&mint);
    let bridge_pda = derive_bridge_pda(&mint, client_id);
    let sender_ata = token_kind.get_ata(&sender, &mint);

    let (gmp_app_state_pda, _) =
        Pubkey::find_program_address(&[ics27_gmp::state::GMPAppState::SEED], &ics27_gmp::ID);
    let (router_state_pda, _) =
        Pubkey::find_program_address(&[RouterState::SEED], &ics26_router::ID);
    let (commitment_pda, _) = Pubkey::find_program_address(
        &[
            Commitment::PACKET_COMMITMENT_SEED,
            client_id.as_bytes(),
            &params.sequence.to_le_bytes(),
        ],
        &ics26_router::ID,
    );
    let (ibc_app_pda, _) =
        Pubkey::find_program_address(&[IBCApp::SEED, GMP_PORT_ID.as_bytes()], &ics26_router::ID);
    let (client_pda, _) =
        Pubkey::find_program_address(&[Client::SEED, client_id.as_bytes()], &ics26_router::ID);
    let pending_transfer_pda = derive_pending_transfer_pda(&mint, client_id, params.sequence);

    let msg = ift::state::IFTTransferMsg {
        client_id: client_id.to_string(),
        receiver: params.receiver,
        amount: params.amount,
        timeout_timestamp: params.timeout_timestamp,
        sequence: params.sequence,
    };

    let ix = Instruction {
        program_id: ift::ID,
        accounts: vec![
            AccountMeta::new_readonly(app_state_pda, false),
            AccountMeta::new_readonly(app_mint_state_pda, false),
            AccountMeta::new_readonly(bridge_pda, false),
            AccountMeta::new(mint, false),
            AccountMeta::new(sender_ata, false),
            AccountMeta::new_readonly(sender, true),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(token_kind.program_id(), false),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(ics27_gmp::ID, false),
            AccountMeta::new(gmp_app_state_pda, false),
            AccountMeta::new_readonly(ics26_router::ID, false),
            AccountMeta::new_readonly(router_state_pda, false),
            AccountMeta::new(commitment_pda, false),
            AccountMeta::new_readonly(ibc_app_pda, false),
            AccountMeta::new_readonly(client_pda, false),
            AccountMeta::new_readonly(mock_light_client::ID, false),
            AccountMeta::new_readonly(mock_client_state, false),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            AccountMeta::new_readonly(mock_consensus_state, false),
            AccountMeta::new(pending_transfer_pda, false),
        ],
        data: ift::instruction::IftTransfer { msg }.data(),
    };

    IftTransferResult {
        ix,
        commitment_pda,
        pending_transfer_pda,
    }
}

// ── Finalize Transfer ───────────────────────────────────────────────────

/// Build a `finalize_transfer` instruction to complete or refund a transfer
/// after the GMP result is available.
pub fn build_finalize_transfer_ix(
    payer: Pubkey,
    mint: Pubkey,
    sender: Pubkey,
    client_id: &str,
    sequence: u64,
    token_kind: TokenKind,
) -> Instruction {
    let app_state_pda = derive_app_state_pda();
    let app_mint_state_pda = derive_app_mint_state_pda(&mint);
    let pending_transfer_pda = derive_pending_transfer_pda(&mint, client_id, sequence);
    let mint_authority_pda = derive_mint_authority_pda(&mint);
    let sender_ata = token_kind.get_ata(&sender, &mint);

    let (gmp_result_pda, _) =
        solana_ibc_types::GMPCallResult::pda(client_id, sequence, &ics27_gmp::ID);

    Instruction {
        program_id: ift::ID,
        accounts: vec![
            AccountMeta::new_readonly(app_state_pda, false),
            AccountMeta::new(app_mint_state_pda, false),
            AccountMeta::new(pending_transfer_pda, false),
            AccountMeta::new_readonly(gmp_result_pda, false),
            AccountMeta::new(mint, false),
            AccountMeta::new_readonly(mint_authority_pda, false),
            AccountMeta::new(sender_ata, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(token_kind.program_id(), false),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
        ],
        data: ift::instruction::FinalizeTransfer {
            client_id: client_id.to_string(),
            sequence,
        }
        .data(),
    }
}

// ── GMP ack/timeout param structs ───────────────────────────────────────

/// Parameters for building an IFT GMP `ack_packet` instruction (ABI encoding).
pub struct IftGmpAckPacketParams {
    /// Packet sequence number.
    pub sequence: u64,
    /// Raw acknowledgement bytes.
    pub acknowledgement: Vec<u8>,
    /// PDA of the uploaded payload chunk.
    pub payload_chunk_pda: Pubkey,
    /// PDA of the uploaded proof chunk.
    pub proof_chunk_pda: Pubkey,
}

/// Parameters for building an IFT GMP `timeout_packet` instruction (ABI encoding).
pub struct IftGmpTimeoutPacketParams {
    /// Packet sequence number.
    pub sequence: u64,
    /// PDA of the uploaded payload chunk.
    pub payload_chunk_pda: Pubkey,
    /// PDA of the uploaded proof chunk.
    pub proof_chunk_pda: Pubkey,
}

// ── GMP ack/timeout builders with ABI encoding ─────────────────────────

/// Build a GMP `ack_packet` instruction for an IFT transfer (ABI encoding).
///
/// Returns `(instruction, commitment_pda)`.
pub fn build_ift_gmp_ack_packet_ix(
    relayer: Pubkey,
    source_client: &str,
    dest_client: &str,
    clock_time: i64,
    params: IftGmpAckPacketParams,
) -> (Instruction, Pubkey) {
    let gmp_app_state_pda =
        Pubkey::find_program_address(&[ics27_gmp::state::GMPAppState::SEED], &ics27_gmp::ID).0;
    let (result_pda, _) =
        solana_ibc_types::GMPCallResult::pda(source_client, params.sequence, &ics27_gmp::ID);

    crate::router::build_ack_packet_ix(
        relayer,
        source_client,
        dest_client,
        clock_time,
        crate::router::AckPacketParams {
            sequence: params.sequence,
            acknowledgement: params.acknowledgement,
            payload_chunk_pda: params.payload_chunk_pda,
            proof_chunk_pda: params.proof_chunk_pda,
            port_id: GMP_PORT_ID,
            version: ICS27_VERSION,
            encoding: ICS27_ENCODING_ABI,
            app_program: ics27_gmp::ID,
            app_state_pda: gmp_app_state_pda,
            extra_remaining_accounts: vec![AccountMeta::new(result_pda, false)],
        },
    )
}

/// Build a GMP `timeout_packet` instruction for an IFT transfer (ABI encoding).
///
/// Returns `(instruction, commitment_pda)`.
pub fn build_ift_gmp_timeout_packet_ix(
    relayer: Pubkey,
    source_client: &str,
    dest_client: &str,
    clock_time: i64,
    params: IftGmpTimeoutPacketParams,
) -> (Instruction, Pubkey) {
    let gmp_app_state_pda =
        Pubkey::find_program_address(&[ics27_gmp::state::GMPAppState::SEED], &ics27_gmp::ID).0;
    let (result_pda, _) =
        solana_ibc_types::GMPCallResult::pda(source_client, params.sequence, &ics27_gmp::ID);

    crate::router::build_timeout_packet_ix(
        relayer,
        source_client,
        dest_client,
        clock_time,
        crate::router::TimeoutPacketParams {
            sequence: params.sequence,
            payload_chunk_pda: params.payload_chunk_pda,
            proof_chunk_pda: params.proof_chunk_pda,
            port_id: GMP_PORT_ID,
            version: ICS27_VERSION,
            encoding: ICS27_ENCODING_ABI,
            app_program: ics27_gmp::ID,
            app_state_pda: gmp_app_state_pda,
            extra_remaining_accounts: vec![AccountMeta::new(result_pda, false)],
        },
    )
}

// ── Ack helpers ─────────────────────────────────────────────────────────

/// Standard IBC success acknowledgement byte (`[1]`).
pub fn success_ack() -> Vec<u8> {
    vec![1]
}

/// Universal error acknowledgement: `sha256("UNIVERSAL_ERROR_ACKNOWLEDGEMENT")`.
pub fn universal_error_ack() -> Vec<u8> {
    solana_ibc_types::UNIVERSAL_ERROR_ACK.to_vec()
}

// ── Payload encoding ────────────────────────────────────────────────────

/// Encode the `iftMint(address, uint256)` ABI call payload.
///
/// This replicates what the IFT program does internally in
/// `construct_evm_mint_call`.
pub fn encode_evm_mint_call(receiver: &str, amount: u64) -> Vec<u8> {
    alloy_sol_types::sol! {
        function iftMint(address receiver, uint256 amount);
    }
    use alloy_sol_types::private::{Address, U256};
    use alloy_sol_types::SolCall;

    let receiver: Address = receiver.parse().expect("valid EVM address");
    iftMintCall {
        receiver,
        amount: U256::from(amount),
    }
    .abi_encode()
}

/// Encode a complete GMP packet for an IFT transfer using ABI encoding.
///
/// The packet wraps the `iftMint` payload with the IFT program as sender
/// and the counterparty IFT contract as receiver.
pub fn encode_ift_gmp_packet(counterparty_addr: &str, mint_call_payload: Vec<u8>) -> Vec<u8> {
    let raw_packet = solana_ibc_proto::RawGmpPacketData {
        sender: ift::ID.to_string(),
        receiver: counterparty_addr.to_string(),
        salt: vec![],
        payload: mint_call_payload,
        memo: String::new(),
    };

    ics27_gmp::encoding::encode_gmp_packet(
        solana_ibc_types::GmpPacketData::try_from(raw_packet).expect("valid GMP packet data"),
        ICS27_ENCODING_ABI,
    )
    .expect("GMP packet encoding should succeed")
}

// ── Account state readers ───────────────────────────────────────────────

/// Deserialize a `PendingTransfer` from its PDA.
pub async fn read_pending_transfer(chain: &Chain, pda: Pubkey) -> ift::state::PendingTransfer {
    use anchor_lang::AccountDeserialize;
    let account = chain
        .get_account(pda)
        .await
        .expect("PendingTransfer should exist");
    ift::state::PendingTransfer::try_deserialize(&mut &account.data[..])
        .expect("deserialize PendingTransfer")
}

/// Deserialize the on-chain `IFTAppState` from its PDA.
pub async fn read_app_state(chain: &Chain) -> ift::state::IFTAppState {
    use anchor_lang::AccountDeserialize;
    let pda = derive_app_state_pda();
    let account = chain
        .get_account(pda)
        .await
        .expect("IFTAppState should exist");
    ift::state::IFTAppState::try_deserialize(&mut &account.data[..])
        .expect("deserialize IFTAppState")
}

/// Assert that a `PendingTransfer` account has been closed.
pub async fn assert_pending_transfer_closed(chain: &Chain, pda: Pubkey) {
    assert!(
        chain.get_account(pda).await.is_none(),
        "PendingTransfer should be closed"
    );
}

// ── Admin instruction builders ──────────────────────────────────────────

/// Build an IFT `propose_admin` instruction.
pub fn build_propose_admin_ix(admin: Pubkey, new_admin: Pubkey) -> Instruction {
    let app_state_pda = derive_app_state_pda();

    Instruction {
        program_id: ift::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new_readonly(admin, true),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
        ],
        data: ift::instruction::ProposeAdmin { new_admin }.data(),
    }
}

/// Build an IFT `accept_admin` instruction.
pub fn build_accept_admin_ix(pending_admin: Pubkey) -> Instruction {
    let app_state_pda = derive_app_state_pda();

    Instruction {
        program_id: ift::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new_readonly(pending_admin, true),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
        ],
        data: ift::instruction::AcceptAdmin {}.data(),
    }
}

/// Build an IFT `cancel_admin_proposal` instruction.
pub fn build_cancel_admin_proposal_ix(admin: Pubkey) -> Instruction {
    let app_state_pda = derive_app_state_pda();

    Instruction {
        program_id: ift::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new_readonly(admin, true),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
        ],
        data: ift::instruction::CancelAdminProposal {}.data(),
    }
}

/// Build an IFT `set_paused` instruction.
pub fn build_set_paused_ix(admin: Pubkey, paused: bool) -> Instruction {
    let app_state_pda = derive_app_state_pda();

    Instruction {
        program_id: ift::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new_readonly(admin, true),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
        ],
        data: ift::instruction::SetPaused {
            msg: ift::state::SetPausedMsg { paused },
        }
        .data(),
    }
}
