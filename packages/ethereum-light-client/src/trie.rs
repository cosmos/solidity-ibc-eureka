use alloy_primitives::B256;
use sha2::{Digest, Sha256};

use crate::error::{EthereumIBCError, InvalidMerkleBranch};
// https://github.com/ethereum/consensus-specs/blob/efb554f4c4848f8bfc260fcf3ff4b806971716f6/specs/phase0/beacon-chain.md#is_valid_merkle_branch
pub fn validate_merkle_branch(
    leaf: B256,
    branch: Vec<B256>,
    depth: usize,
    index: u64,
    root: B256,
) -> Result<(), EthereumIBCError> {
    let mut value = leaf;
    for (i, branch_node) in branch.iter().take(depth).enumerate() {
        if (index / 2u64.checked_pow(i as u32).unwrap()) % 2 != 0 {
            let mut hasher = Sha256::new();
            hasher.update(branch_node);
            hasher.update(value);

            value = B256::from_slice(&hasher.finalize()[..]);
        } else {
            let mut hasher = Sha256::new();
            hasher.update(value);
            hasher.update(branch_node);

            value = B256::from_slice(&hasher.finalize()[..]);
        }
    }

    if value == root {
        Ok(())
    } else {
        Err(EthereumIBCError::InvalidMerkleBranch(InvalidMerkleBranch {
            leaf,
            branch,
            depth,
            index,
            root,
            found: value,
        }))
    }
}

#[cfg(test)]
mod test {

    use alloy_primitives::{hex::FromHex, Address, Bloom, Bytes, B256, U256};

    use crate::{
        client_state::ClientState,
        config::{
            consts::{floorlog2, get_subtree_index, EXECUTION_PAYLOAD_INDEX},
            MINIMAL,
        },
        trie::validate_merkle_branch,
        types::{
            light_client::{BeaconBlockHeader, ExecutionPayloadHeader, LightClientHeader},
            wrappers::{MyBloom, MyBranch, MyBytes},
        },
        verify::get_lc_execution_root,
    };

    #[test]
    fn test_validate_merkle_branch_with_execution_payload() {
        let header = LightClientHeader {
            beacon: BeaconBlockHeader {
                slot: 10000,
                proposer_index: 0,
                parent_root: B256::default(),
                state_root: B256::default(),
                body_root: B256::from_hex(
                    "0x045a26b541713c820616774b2082317cdd74dcff424c255c803e558843e55371",
                )
                .unwrap(),
            },
            execution: ExecutionPayloadHeader {
                parent_hash: B256::from_hex(
                    "f55156c2b27326547193bcd2501c8300a0f3617a7d71f096fc992955f042ea50",
                )
                .unwrap(),
                fee_recipient: Address::from_hex("0x8943545177806ED17B9F23F0a21ee5948eCaa776").unwrap(),
                state_root: B256::from_hex(
                    "47baba45d0ee0f0abaa42d7fbdba87908052d81fe33806576215bcf136167510",
                )
                .unwrap(),
                receipts_root: B256::from_hex(
                    "56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421",
                ).unwrap(),
                logs_bloom: MyBloom(Bloom::from_hex("0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").unwrap()),
                prev_randao: B256::from_hex("707a729f27185bfd88c746532e0909f7f4604dc5b25b6d9ffb5cfec6ca7987d9").unwrap(),
                block_number: 80,
                gas_limit: 30000000,
                gas_used: 0,
                timestamp: 1732901097,
                extra_data: MyBytes(Bytes::from_hex("0xd883010e06846765746888676f312e32322e34856c696e7578").unwrap()),
                base_fee_per_gas: U256::from(27136),
                block_hash: B256::from_hex("c001e15851608006eb33999e829bb265706929091f4c9a08f6853f6fbe96a730").unwrap(),
                transactions_root: B256::from_hex("0x7ffe241ea60187fdb0187bfa22de35d1f9bed7ab061d9401fd47e34a54fbede1").unwrap(),
                withdrawals_root: B256::from_hex("0x28ba1834a3a7b657460ce79fa3a1d909ab8828fd557659d4d0554a9bdbc0ec30").unwrap(),
                blob_gas_used: 0,
                excess_blob_gas: 0,
            },
            execution_branch: MyBranch([
                B256::from_hex("0xd320d2b395e1065b0b2e3dbb7843c6d77cb7830ef340ffc968caa0f92e26f080")
                    .unwrap(),
                B256::from_hex("0x6c6dd63656639d153a2e86a9cab291e7a26e957ad635fec872d2836e92340c23")
                    .unwrap(),
                B256::from_hex("0xdb56114e00fdd4c1f85c892bf35ac9a89289aaecb1ebd0a96cde606a748b5d71")
                    .unwrap(),
                B256::from_hex("0xee70868f724f428f301007b0967c82d9c31fb5fd549d7f25342605169b90a3d6")
                    .unwrap(),
            ]),
        };

        // inputs
        let leaf = get_lc_execution_root(
            &ClientState {
                slots_per_epoch: 32,
                fork_parameters: MINIMAL.fork_parameters,
                ..Default::default()
            },
            &header,
        );
        let depth = floorlog2(EXECUTION_PAYLOAD_INDEX);
        let index = get_subtree_index(EXECUTION_PAYLOAD_INDEX);
        let root = header.beacon.body_root;

        println!("Leaf: {:?}", leaf);
        println!("Branch: {:?}", header.execution_branch);
        println!("Depth: {:?}", depth);
        println!("Index: {:?}", index);
        println!("Root: {:?}", root);

        validate_merkle_branch(leaf, header.execution_branch.0.into(), depth, index, root).unwrap();
    }
}
