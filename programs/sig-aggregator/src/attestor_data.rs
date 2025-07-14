use crate::{
    rpc::{
        AggregateResponse, AttestationsFromHeightResponse, SigPubkeyPair,
    },
};
use alloy_primitives::FixedBytes;
use std::collections::HashMap;

type Height = u64;

pub const STATE_BYTE_LENGTH: usize = 32; // Length of the state hash
type State = FixedBytes<STATE_BYTE_LENGTH>;

// https://docs.rs/secp256k1/latest/secp256k1/ecdsa/struct.Signature.html#method.serialize_compact
pub const SIGNATURE_BYTE_LENGTH: usize = 64;
type Signature = FixedBytes<SIGNATURE_BYTE_LENGTH>;

// Compressed public key length
// https://docs.rs/secp256k1/latest/secp256k1/struct.PublicKey.html#method.serialize
pub const PUBKEY_BYTE_LENGTH: usize = 33; 
type Pubkey = FixedBytes<PUBKEY_BYTE_LENGTH>;

//  HashMap<height, HashMap<State, Vec[(Signatures, pub_key)]>>
//  Height: 101
//      State: 0x1234... (32 bytes)
//          Sign_PK: [(SigAtt_A, PK_Att_A), (SigAtt_B, PK_Att_B)]
//      State: 0x9876...
//          Sign_PK: [(SigAtt_C, PK_Att_C), (SigAtt_D, PK_Att_D), (SigAtt_E, PK_Att_E)]
//  Height: 102
//      State: 0x5678...
//          Sign_PK: [(SigAtt_A, PK_Att_A), (SigAtt_B, PK_Att_B), (SigAtt_C, PK_Att_C), (SigAtt_D, PK_Att_D), (SigAtt_E, PK_Att_E)]
type SignedStates = HashMap<State, Vec<(Signature, Pubkey)>>;

#[derive(Debug, Clone)]
pub struct AttestatorData(HashMap<Height, SignedStates>);

impl AttestatorData {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn insert(&mut self, att_resp: AttestationsFromHeightResponse) {
        for attestations in att_resp.attestations {
            let state_map = self.0.entry(attestations.height).or_default();
            state_map
                .entry(State::from_slice(&attestations.data))
                .or_default()
                .push((
                    Signature::from_slice(&attestations.signature), 
                    Pubkey::from_slice(&att_resp.pubkey)
                ));
        }
    }
    
    pub fn get_latest(&self, quorum: usize) -> Option<AggregateResponse> {
        let mut latest = AggregateResponse {
            height: 0,
            state: vec![],
            sig_pubkey_pairs: vec![],
        };

        for (height, state_map) in self.0.iter() {
            if *height <= latest.height {
                continue;
            }

            for (state, sig_to_pks) in state_map.iter() {
                if sig_to_pks.len() < quorum {
                    continue;
                }
            
                latest.height = *height;
                latest.state = state.to_vec();
                latest.sig_pubkey_pairs = sig_to_pks
                    .iter()
                    .map(|(sig, pubkey)| SigPubkeyPair {
                        sig: sig.to_vec(), 
                        pubkey: pubkey.to_vec(),
                    })
                    .collect();
            }
        }

        if latest.height > 0 {
            return Some(latest);
        }
        None
    }
}

impl Default for AttestatorData {
    fn default() -> Self {
        Self::new()
    }
}