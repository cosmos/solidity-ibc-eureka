use ibc_proto_eureka::ibc::core::{channel::v2::MsgRecvPacket, client::v1::Height};

/// Injects mock proofs into the provided messages for testing purposes.
pub fn inject_proofs(recv_msgs: &mut [MsgRecvPacket], attested_packets: &[u8], height: u64) {
    for msg in recv_msgs.iter_mut() {
        msg.proof_commitment = attested_packets.to_vec();
        msg.proof_height = Some(Height {
            revision_height: height,
            ..Default::default()
        });
    }
}
