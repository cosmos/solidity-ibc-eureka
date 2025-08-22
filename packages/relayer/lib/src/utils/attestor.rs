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
