use alloy::primitives::Bytes;
use ibc_eureka_solidity_types::ics26::router::routerCalls;
use ibc_proto_eureka::ibc::core::{channel::v2::MsgRecvPacket, client::v1::Height};

/// Injects the `proofs` for a given height into
/// [`MsgRecvPacket`] msgs
pub fn inject_proofs_for_tm_msg(recv_msgs: &mut [MsgRecvPacket], proof: &[u8], height: u64) {
    for msg in recv_msgs.iter_mut() {
        msg.proof_commitment = proof.to_vec();
        msg.proof_height = Some(Height {
            revision_height: height,
            ..Default::default()
        });
    }
}

/// Injects the `proofs` for a given height into [`MsgRecvPacket`] msgs.
/// The height must be provided
pub fn inject_proofs_for_evm_msg(recv_msgs: &mut [routerCalls], proof: &[u8]) {
    for msg in recv_msgs.iter_mut() {
        match msg {
            routerCalls::recvPacket(call) => call.msg_.proofCommitment = Bytes::from_iter(proof),
            _ => continue,
        }
    }
}
