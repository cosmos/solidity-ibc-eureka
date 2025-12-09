use std::io::{self, Read};

use anyhow::{Context, Result};
use borsh::BorshSerialize;
use ibc_client_tendermint::types::Misbehaviour;
use ibc_proto::ibc::lightclients::tendermint::v1::Misbehaviour as RawMisbehaviour;
use prost::Message;
use solana_ibc_types::borsh_header::conversions::misbehaviour_to_borsh;

fn main() -> Result<()> {
    let mut input = Vec::new();
    io::stdin()
        .read_to_end(&mut input)
        .context("Failed to read from stdin")?;

    let raw = RawMisbehaviour::decode(input.as_slice())
        .context("Failed to decode protobuf Misbehaviour")?;

    let misbehaviour =
        Misbehaviour::try_from(raw).context("Failed to convert RawMisbehaviour to Misbehaviour")?;

    let borsh_misbehaviour = misbehaviour_to_borsh(&misbehaviour);

    let borsh_bytes = borsh_misbehaviour
        .try_to_vec()
        .context("Failed to serialize to Borsh")?;

    println!("{}", hex::encode(borsh_bytes));

    Ok(())
}
