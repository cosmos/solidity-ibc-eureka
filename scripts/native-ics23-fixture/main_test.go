package main

import "testing"

func TestBuildNativeRouterFixture(t *testing.T) {
	fix, err := buildNativeRouterFixture()
	if err != nil {
		t.Fatalf("buildNativeRouterFixture() error = %v", err)
	}

	if fix.Root != "0xff84dcf807489080d308574a3e144e5c9f346cf4f3bf697566842dd5f4bc3c62" {
		t.Fatalf("unexpected router root: %s", fix.Root)
	}
	if fix.ProofHeight != 37 {
		t.Fatalf("unexpected proof height: %d", fix.ProofHeight)
	}
	if len(fix.PacketCommitment.Path) != 2 || len(fix.AcknowledgementCommitment.Path) != 2 || len(fix.PacketReceipt.Path) != 2 {
		t.Fatal("router proofs must use two path segments")
	}
	if fix.PacketCommitment.Value == "" || fix.AcknowledgementCommitment.Value == "" {
		t.Fatal("membership proofs must include expected values")
	}
	if fix.PacketReceipt.Value != "" {
		t.Fatal("non-membership proof must not include a value")
	}
	if fix.PacketCommitment.Proof == "" || fix.AcknowledgementCommitment.Proof == "" || fix.PacketReceipt.Proof == "" {
		t.Fatal("router proofs must be ABI encoded")
	}
}

func TestBuildNativeRouterFixtureFromE2ESources(t *testing.T) {
	fix, err := buildNativeRouterFixtureFromSources(defaultPacketSource, defaultAckSource, defaultReceiptSource)
	if err != nil {
		t.Fatalf("buildNativeRouterFixtureFromSources() error = %v", err)
	}

	if fix.PacketCommitment.ProofHeight == 0 || fix.AcknowledgementCommitment.ProofHeight == 0 || fix.PacketReceipt.ProofHeight == 0 {
		t.Fatal("e2e router proofs must carry explicit proof heights")
	}
	if fix.PacketCommitment.Root == fix.AcknowledgementCommitment.Root {
		t.Fatal("packet and acknowledgement e2e proofs should preserve their distinct consensus roots")
	}
	if fix.AcknowledgementCommitment.Root == "" || fix.PacketReceipt.Root == "" {
		t.Fatal("acknowledgement and receipt proofs must preserve their source consensus roots")
	}
	if fix.Acknowledgement != "0x7b22726573756c74223a2241513d3d227d" {
		t.Fatalf("unexpected application acknowledgement: %s", fix.Acknowledgement)
	}
	if fix.AcknowledgementCommitment.Value != hexBytes(acknowledgementCommitment([]byte(`{"result":"AQ=="}`))) {
		t.Fatalf("acknowledgement commitment does not match app acknowledgement: %s", fix.AcknowledgementCommitment.Value)
	}
	if len(fix.PacketCommitment.Path) != 2 || len(fix.AcknowledgementCommitment.Path) != 2 || len(fix.PacketReceipt.Path) != 2 {
		t.Fatal("e2e router proofs must use ibc/store two-segment paths")
	}
	if fix.Packet.Sequence != 1 || fix.LocalPacket.Sequence != 1 {
		t.Fatalf("unexpected packet sequence: packet=%d local=%d", fix.Packet.Sequence, fix.LocalPacket.Sequence)
	}
	if fix.Packet.Payload.Encoding != "application/json" || fix.LocalPacket.Payload.Encoding != "application/json" {
		t.Fatalf("unexpected packet encodings: packet=%s local=%s", fix.Packet.Payload.Encoding, fix.LocalPacket.Payload.Encoding)
	}
}
