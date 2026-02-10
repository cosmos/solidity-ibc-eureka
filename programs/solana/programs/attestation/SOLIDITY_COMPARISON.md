# Attestation Light Client: Solidity vs Solana Comparison

This document compares the Solidity [`AttestationLightClient.sol`](../../../../contracts/light-clients/attestation/AttestationLightClient.sol) with the Solana [`attestation`](src/lib.rs) Anchor program instruction by instruction.

---

## 1. Initialize / Constructor

### 1.1 Input validation

**Solidity** ([`AttestationLightClient.sol:44-48`](../../../../contracts/light-clients/attestation/AttestationLightClient.sol#L44-L48))
```solidity
require(attestorAddresses.length > 0, NoAttestors());
require(
    minRequiredSigs > 0 && attestorAddresses.length > minRequiredSigs - 1,
    BadQuorum(minRequiredSigs, attestorAddresses.length)
);
```

**Solana** ([`initialize.rs:49-56`](src/instructions/initialize.rs#L49-L56))
```rust
require!(!client_id.is_empty(), ErrorCode::InvalidClientId);
require!(!attestor_addresses.is_empty(), ErrorCode::NoAttestors);
require!(
    min_required_sigs > 0 && (min_required_sigs as usize) <= attestor_addresses.len(),
    ErrorCode::BadQuorum
);
require!(latest_height > 0, ErrorCode::InvalidHeight);
require!(timestamp > 0, ErrorCode::InvalidTimestamp);
```

**Same:** Both require at least one attestor and validate quorum (`min_required_sigs > 0` and `<= attestor count`).

**Difference:** Solana additionally validates `client_id` non-empty, `height > 0` and `timestamp > 0`. Solidity accepts zero height/timestamp.

**Rationale:** Solana needs a non-empty `client_id` because it is used as a PDA seed; an empty string would create an ambiguous address. The `height > 0` and `timestamp > 0` guards prevent nonsensical initial consensus state that would confuse later misbehaviour detection (timestamp `0` is the sentinel for "no consensus state exists").

### 1.2 Duplicate attestor check

**Solidity** ([`AttestationLightClient.sol:57-61`](../../../../contracts/light-clients/attestation/AttestationLightClient.sol#L57-L61))
```solidity
for (uint256 i = 0; i < attestorAddresses.length; ++i) {
    address attestor = attestorAddresses[i];
    require(_isAttestor[attestor] == false, DuplicateSigner(attestor));
    _isAttestor[attestor] = true;
}
```

**Solana** ([`initialize.rs:59-65`](src/instructions/initialize.rs#L59-L65))
```rust
let has_duplicates = attestor_addresses.iter().enumerate().any(|(i, addr)| {
    attestor_addresses
        .iter()
        .skip(i.saturating_add(1))
        .any(|other| addr == other)
});
require!(!has_duplicates, ErrorCode::DuplicateSigner);
```

**Same:** Both reject duplicate attestor addresses during initialization.

**Difference:** Solidity uses a mapping for O(n) total. Solana uses nested iteration for O(n^2).

**Rationale:** Solidity naturally has `_isAttestor` mapping available (used later for O(1) attestor lookups during verification). Solana stores attestors as a Vec without a separate index, so pairwise comparison is the simplest approach. The attestor list is capped at 20 entries, making O(n^2) negligible.

### 1.3 State storage

**Solidity** ([`AttestationLightClient.sol:50-63`](../../../../contracts/light-clients/attestation/AttestationLightClient.sol#L50-L63))
```solidity
clientState = IAttestationLightClientMsgs.ClientState({
    attestorAddresses: attestorAddresses,
    minRequiredSigs: minRequiredSigs,
    latestHeight: initialHeight,
    isFrozen: false
});
_consensusTimestampAtHeight[initialHeight] = initialTimestampSeconds;
```

**Solana** ([`initialize.rs:67-80`](src/instructions/initialize.rs#L67-L80))
```rust
client_state_account.client_id = client_id;
client_state_account.attestor_addresses = attestor_addresses;
client_state_account.min_required_sigs = min_required_sigs;
client_state_account.latest_height = latest_height;
client_state_account.is_frozen = false;

consensus_state_store.height = latest_height;
consensus_state_store.consensus_state = ConsensusState { height: latest_height, timestamp };
```

**Same:** Both persist the same fields: attestor addresses, quorum threshold, latest height, frozen flag and initial consensus state.

**Difference:** Solana stores a full `ConsensusState` struct (height + timestamp) in a PDA account. Solidity stores only `timestamp` in a `mapping(uint64 => uint64)`.

**Rationale:** Solana accounts are addressed by PDA seeds (which include the height), so each height maps to a distinct account holding the full consensus state. This follows the same `ConsensusStateStore` layout used by the ICS07 Tendermint light client. Solidity uses a simple mapping keyed by height since EVM storage slots are cheap to index.

### 1.4 Account versioning and reserved space

**Solidity:** No equivalent.

**Solana** ([`initialize.rs:68`](src/instructions/initialize.rs#L68), [`82-85`](src/instructions/initialize.rs#L82-L85))
```rust
client_state_account.version = AccountVersion::V1;
// ...
app_state.version = AccountVersion::V1;
app_state._reserved = [0; 256];
```

**Difference:** Solana sets an `AccountVersion::V1` field on both `ClientState` and `AppState`, and reserves 256 bytes in `AppState` for future upgrades. Solidity has neither.

**Rationale:** Solana account data has a fixed layout determined at creation. The version field enables future account migrations and the reserved space allows adding fields without reallocating the account.

### 1.5 Access control setup

**Solidity** ([`AttestationLightClient.sol:65-70`](../../../../contracts/light-clients/attestation/AttestationLightClient.sol#L65-L70))
```solidity
if (roleManager == address(0)) {
    _grantRole(PROOF_SUBMITTER_ROLE, address(0)); // allow anyone
} else {
    _grantRole(DEFAULT_ADMIN_ROLE, roleManager);
    _grantRole(PROOF_SUBMITTER_ROLE, roleManager);
}
```

**Solana** ([`initialize.rs:84`](src/instructions/initialize.rs#L84))
```rust
app_state.access_manager = access_manager;
```

**Same:** Both configure an access control authority during initialization.

**Difference:** Solidity uses OpenZeppelin `AccessControl` with inline `PROOF_SUBMITTER_ROLE`. Solana delegates to an external `access_manager` program with `RELAYER_ROLE`.

**Rationale:** Solidity inherits OpenZeppelin's `AccessControl` with a dedicated `PROOF_SUBMITTER_ROLE`. Solana has no contract inheritance, so it uses a shared external `access-manager` program with a general-purpose `RELAYER_ROLE`.

---

## 2. Update Client

### 2.1 Access control and frozen check

**Solidity** ([`AttestationLightClient.sol:89-94`](../../../../contracts/light-clients/attestation/AttestationLightClient.sol#L89-L94), [`251-262`](../../../../contracts/light-clients/attestation/AttestationLightClient.sol#L251-L262))
```solidity
modifier notFrozen() {
    require(!clientState.isFrozen, FrozenClientState());
    _;
}
modifier onlyProofSubmitter() {
    if (!hasRole(PROOF_SUBMITTER_ROLE, address(0))) {
        _checkRole(PROOF_SUBMITTER_ROLE);
    }
    _;
}
function updateClient(bytes calldata updateMsg)
    external notFrozen onlyProofSubmitter
    returns (ILightClientMsgs.UpdateResult)
```

**Solana** ([`update_client.rs:63-73`](src/instructions/update_client.rs#L63-L73))
```rust
access_manager::require_role(
    &ctx.accounts.access_manager,
    solana_ibc_types::roles::RELAYER_ROLE,
    &ctx.accounts.submitter,
    &ctx.accounts.instructions_sysvar,
    &crate::ID,
)?;
require!(!client_state.is_frozen, ErrorCode::FrozenClientState);
```

**Same:** Both guard `update_client` behind a role check and a frozen-state check.

**Difference:** Solidity checks frozen first (modifier order), then role. Solana checks role first, then frozen.

**Rationale:** Solidity modifier ordering is declarative; `notFrozen` appears first in the function signature. Solana uses imperative checks, and verifying the caller's role first avoids loading state for unauthorized callers. The ordering has no semantic impact.

### 2.2 Client identity constraint

**Solidity:** No equivalent — each contract deployment is a single client instance.

**Solana** ([`update_client.rs:13-16`](src/instructions/update_client.rs#L13-L16))
```rust
#[account(
    mut,
    constraint = client_state.client_id == client_id,
)]
pub client_state: Account<'info, ClientState>,
```

**Difference:** Solana requires a `client_id` instruction argument that must match the stored `client_id` on the account. This ensures the instruction targets the correct client.

**Rationale:** A single Solana program serves multiple clients (each with its own PDA). The constraint prevents operating on the wrong client state account. Solidity deploys a separate contract per client, so identity is implicit in the contract address.

### 2.3 Proof decoding and signature verification

**Solidity** ([`AttestationLightClient.sol:95-101`](../../../../contracts/light-clients/attestation/AttestationLightClient.sol#L95-L101))
```solidity
IAttestationMsgs.AttestationProof memory proof = abi.decode(updateMsg, (IAttestationMsgs.AttestationProof));
bytes32 digest = sha256(proof.attestationData);
_verifySignaturesThreshold(digest, proof.signatures);

IAttestationMsgs.StateAttestation memory state =
    abi.decode(proof.attestationData, (IAttestationMsgs.StateAttestation));
```

**Solana** ([`update_client.rs:75-78`](src/instructions/update_client.rs#L75-L78))
```rust
let proof = deserialize_membership_proof(&params.proof)?;
let attestation = decode_state_attestation(&proof.attestation_data)?;

verify_attestation(client_state, &proof.attestation_data, &proof.signatures)?;
```

**Same:** Both decode the outer proof envelope, decode the inner attestation data, compute SHA-256 digest and verify signatures against the attestor quorum.

**Difference:** Solidity verifies signatures before decoding the inner attestation data. Solana decodes the attestation data first, then verifies signatures.

**Rationale:** Both reach the same outcome for valid inputs. Solana ordering lets it fail faster on malformed attestation data (cheaper than signature recovery). Solidity's approach means the inner attestation bytes are only interpreted after signatures are confirmed authentic.

### 2.4 State validation

**Solidity** ([`AttestationLightClient.sol:103`](../../../../contracts/light-clients/attestation/AttestationLightClient.sol#L103))
```solidity
require(state.height > 0 && state.timestamp > 0, InvalidState(state.height, state.timestamp));
```

**Solana** ([`update_client.rs:80-85`](src/instructions/update_client.rs#L80-L85))
```rust
require!(attestation.height > 0 && attestation.timestamp > 0, ErrorCode::InvalidState);
require!(new_height == attestation.height, ErrorCode::HeightMismatch);
```

**Same:** Both require that the attested height and timestamp are non-zero.

**Difference:** Solana has an additional check ensuring the instruction argument `new_height` matches the height inside the proof.

**Rationale:** Solana's consensus state is stored in a PDA whose seeds include the height. The `new_height` argument determines which PDA to create/load, so it must match the proof's height to prevent writing consensus state to the wrong account.

### 2.5 Misbehaviour detection

**Solidity** ([`AttestationLightClient.sol:107-114`](../../../../contracts/light-clients/attestation/AttestationLightClient.sol#L107-L114))
```solidity
uint64 existingTimestamp = _consensusTimestampAtHeight[state.height];
if (existingTimestamp != 0) {
    if (existingTimestamp != state.timestamp) {
        clientState.isFrozen = true;
        return ILightClientMsgs.UpdateResult.Misbehaviour;
    }
    return ILightClientMsgs.UpdateResult.NoOp;
}
```

**Solana** ([`update_client.rs:90-102`](src/instructions/update_client.rs#L90-L102))
```rust
let existing_timestamp = consensus_state_store.consensus_state.timestamp;
if existing_timestamp != 0 {
    if existing_timestamp != attestation.timestamp {
        client_state.is_frozen = true;
        emit!(MisbehaviourDetected { ... });
    }
    return Ok(());
}
```

**Same:** Both detect misbehaviour by comparing the new timestamp against any existing timestamp at the same height. Both freeze the client on conflicting timestamps and treat matching timestamps as no-ops.

**Difference:** Solidity returns distinct `UpdateResult` variants (`Misbehaviour`, `NoOp`, `Update`). Solana returns `Ok(())` for all successful cases and emits a `MisbehaviourDetected` event for misbehaviour.

**Rationale:** Solidity's EVM function return values are the standard way to communicate outcomes. Solana CPI does not easily propagate typed return values, so events are the conventional signaling mechanism.

### 2.6 Height and consensus state update

**Solidity** ([`AttestationLightClient.sol:116-121`](../../../../contracts/light-clients/attestation/AttestationLightClient.sol#L116-L121))
```solidity
if (state.height > clientState.latestHeight) {
    clientState.latestHeight = state.height;
}
_consensusTimestampAtHeight[state.height] = state.timestamp;
return ILightClientMsgs.UpdateResult.Update;
```

**Solana** ([`update_client.rs:104-112`](src/instructions/update_client.rs#L104-L112))
```rust
if attestation.height > client_state.latest_height {
    client_state.latest_height = attestation.height;
}
consensus_state_store.height = attestation.height;
consensus_state_store.consensus_state = ConsensusState {
    height: attestation.height,
    timestamp: attestation.timestamp,
};
Ok(())
```

**Same:** Both update `latest_height` only when the new height exceeds the current one (allowing non-sequential updates). Both persist the new consensus state.

**Difference:** Solana writes a full `ConsensusState` struct; Solidity writes only the timestamp to a mapping.

**Rationale:** Same storage model difference as [section 1.3](#13-state-storage).

---

## 3. Verify Membership

### 3.1 Access control and frozen check

**Solidity** ([`AttestationLightClient.sol:125-131`](../../../../contracts/light-clients/attestation/AttestationLightClient.sol#L125-L131))
```solidity
function verifyMembership(ILightClientMsgs.MsgVerifyMembership calldata msg_)
    external view notFrozen onlyProofSubmitter
    returns (uint256)
```

**Solana** ([`verify_membership.rs:29`](src/instructions/verify_membership.rs#L29))
```rust
require!(!client_state.is_frozen, ErrorCode::FrozenClientState);
// No access control - called via CPI from the router
```

**Same:** Both check that the client is not frozen before proceeding.

**Difference:** Solidity applies `onlyProofSubmitter` modifier. Solana has no access control on this instruction.

**Rationale:** On Solana, `verify_membership` is called via CPI from the ICS26 router which itself has role-gated entry points. The router is the only expected caller, so access control at the light client level would be redundant. Solidity applies the modifier uniformly because any external caller can invoke the function directly.

### 3.2 Input validation

**Solidity** ([`AttestationLightClient.sol:132-133`](../../../../contracts/light-clients/attestation/AttestationLightClient.sol#L132-L133))
```solidity
require(msg_.value.length != 0, EmptyValue());
require(msg_.path.length == 1, InvalidPathLength(1, msg_.path.length));
```

**Solana** ([`verify_membership.rs:23-24`](src/instructions/verify_membership.rs#L23-L24))
```rust
require!(!msg.value.is_empty(), ErrorCode::EmptyValue);
require!(msg.path.len() == 1, ErrorCode::InvalidPathLength);
```

**Same:** Both require non-empty value and exactly one path element. Equivalent logic.

### 3.3 Consensus timestamp check

**Solidity** ([`AttestationLightClient.sol:136-138`](../../../../contracts/light-clients/attestation/AttestationLightClient.sol#L136-L138))
```solidity
uint64 proofHeight = msg_.proofHeight.revisionHeight;
uint64 ts = _consensusTimestampAtHeight[proofHeight];
require(ts != 0, ConsensusTimestampNotFound(proofHeight));
```

**Solana** ([`verify_membership.rs:32-35`](src/instructions/verify_membership.rs#L32-L35))
```rust
// Consensus state account is loaded via PDA seeds constraint:
// seeds = [ConsensusStateStore::SEED, client_state.key(), &msg.height.to_le_bytes()]
require!(
    consensus_state_store.consensus_state.timestamp != 0,
    ErrorCode::ConsensusTimestampNotFound
);
```

**Same:** Both ensure a valid consensus state exists at the requested height before proceeding with verification.

**Difference:** Solidity looks up timestamp from a mapping. Solana loads a PDA account derived from the height; if the account doesn't exist, the transaction fails at the account validation level before reaching instruction logic.

**Rationale:** Solana's PDA model provides an implicit existence check (missing account = transaction failure). The explicit `timestamp != 0` check handles the edge case where the account exists but was never properly initialized.

### 3.4 Proof verification

**Solidity** ([`AttestationLightClient.sol:140-149`](../../../../contracts/light-clients/attestation/AttestationLightClient.sol#L140-L149))
```solidity
IAttestationMsgs.AttestationProof memory proof = abi.decode(msg_.proof, (IAttestationMsgs.AttestationProof));
bytes32 digest = sha256(proof.attestationData);
_verifySignaturesThreshold(digest, proof.signatures);

IAttestationMsgs.PacketAttestation memory packetAttestation =
    abi.decode(proof.attestationData, (IAttestationMsgs.PacketAttestation));
require(packetAttestation.height == proofHeight, HeightMismatch(proofHeight, packetAttestation.height));
```

**Solana** ([`verify_membership.rs:37-46`](src/instructions/verify_membership.rs#L37-L46))
```rust
let proof = deserialize_membership_proof(&msg.proof)?;
let attestation = decode_packet_attestation(&proof.attestation_data)?;
require!(attestation.height == consensus_state_store.consensus_state.height, ErrorCode::HeightMismatch);
verify_attestation(client_state, &proof.attestation_data, &proof.signatures)?;
```

**Same:** Both decode the proof, verify signatures against the quorum and check that the attested height matches the expected proof height.

**Difference:** Same ordering difference as `update_client`: Solidity verifies signatures before decoding attestation; Solana decodes first.

**Rationale:** See [section 2.3](#23-proof-decoding-and-signature-verification).

### 3.5 Membership check

**Solidity** ([`AttestationLightClient.sol:152-161`](../../../../contracts/light-clients/attestation/AttestationLightClient.sol#L152-L161))
```solidity
bytes32 pathHash = keccak256(msg_.path[0]);
bytes32 value = abi.decode(msg_.value, (bytes32));
require(packetAttestation.packets.length > 0, EmptyPackets());
for (uint256 i = 0; i < packetAttestation.packets.length; ++i) {
    if (packetAttestation.packets[i].path == pathHash
        && packetAttestation.packets[i].commitment == value) {
        return uint256(ts);
    }
}
revert NotMember();
```

**Solana** ([`verify_membership.rs:48-69`](src/instructions/verify_membership.rs#L48-L69))
```rust
require!(!attestation.packets.is_empty(), ErrorCode::EmptyAttestation);

let Hash(path_hash) = keccak256(&msg.path[0]);

let packet = attestation.packets.iter()
    .find(|p| p.path == path_hash)
    .ok_or_else(|| error!(ErrorCode::NotMember))?;

let value_hash: [u8; 32] = msg.value.as_slice()
    .try_into()
    .map_err(|_| error!(ErrorCode::CommitmentMismatch))?;

require!(packet.commitment == value_hash, ErrorCode::CommitmentMismatch);
Ok(())
```

**Same:** Both hash the path with keccak256, require non-empty packets, search for a matching packet and verify the commitment value.

**Differences:**
- **Matching strategy:** Solidity checks `path == pathHash && commitment == value` in one loop. Solana uses `find` on path first, then checks commitment separately. If duplicate paths existed (invalid attestation), Solidity would continue scanning; Solana stops at the first path match. In practice, duplicate paths represent an invalid attestation from the attestor and should not occur.
- **Error granularity:** Solana distinguishes `NotMember` (path not found) from `CommitmentMismatch` (path found, wrong value). Solidity uses only `NotMember` for both.
- **Return value:** Solidity returns `uint256(ts)` (the consensus timestamp). Solana returns `Ok(())` with no timestamp. The Solana router does not use the timestamp from membership verification (confirmed in [`light_client_cpi.rs:19-32`](../ics26-router/src/router_cpi/light_client_cpi.rs#L19-L32) and matching the Solidity router's [`ICS26Router.sol:173`](../../../../contracts/ICS26Router.sol#L173) `// NOTE: The verification timestamp is not used here` comment).

**Rationale:** The two-step `find`-then-check approach is idiomatic Rust and produces clearer error messages. The return value omission is intentional — the ICS26 router never uses the membership timestamp on either chain.

---

## 4. Verify Non-Membership

### 4.1 Access control and frozen check

**Solidity** ([`AttestationLightClient.sol:165-170`](../../../../contracts/light-clients/attestation/AttestationLightClient.sol#L165-L170))
```solidity
function verifyNonMembership(ILightClientMsgs.MsgVerifyNonMembership calldata msg_)
    external view notFrozen onlyProofSubmitter
    returns (uint256)
```

**Solana** ([`verify_non_membership.rs:32`](src/instructions/verify_non_membership.rs#L32))
```rust
require!(!client_state.is_frozen, ErrorCode::FrozenClientState);
// No access control
```

**Same:** Both check that the client is not frozen.

**Difference:** Same as `verify_membership` — Solidity requires `PROOF_SUBMITTER_ROLE`, Solana does not.

**Rationale:** See [section 3.1](#31-access-control-and-frozen-check).

### 4.2 Input validation

**Solidity** ([`AttestationLightClient.sol:172`](../../../../contracts/light-clients/attestation/AttestationLightClient.sol#L172))
```solidity
require(msg_.path.length == 1, InvalidPathLength(1, msg_.path.length));
```

**Solana** ([`verify_non_membership.rs:27`](src/instructions/verify_non_membership.rs#L27))
```rust
require!(msg.path.len() == 1, ErrorCode::InvalidPathLength);
```

**Same:** Both require exactly one path element. Neither checks for empty value (non-membership has no value). Equivalent logic.

### 4.3 Consensus timestamp check

**Solidity** ([`AttestationLightClient.sol:175-177`](../../../../contracts/light-clients/attestation/AttestationLightClient.sol#L175-L177))
```solidity
uint64 proofHeight = msg_.proofHeight.revisionHeight;
uint64 ts = _consensusTimestampAtHeight[proofHeight];
require(ts != 0, ConsensusTimestampNotFound(proofHeight));
```

**Solana** ([`verify_non_membership.rs:35-38`](src/instructions/verify_non_membership.rs#L35-L38))
```rust
require!(
    consensus_state_store.consensus_state.timestamp != 0,
    ErrorCode::ConsensusTimestampNotFound
);
```

**Same:** Both require an existing non-zero consensus timestamp at the proof height.

**Difference:** Same pattern as verify_membership ([section 3.3](#33-consensus-timestamp-check)). Solidity uses mapping lookup; Solana uses PDA account.

**Rationale:** See [section 3.3](#33-consensus-timestamp-check).

### 4.4 Proof verification

**Solidity** ([`AttestationLightClient.sol:179-188`](../../../../contracts/light-clients/attestation/AttestationLightClient.sol#L179-L188))
```solidity
IAttestationMsgs.AttestationProof memory proof = abi.decode(msg_.proof, (IAttestationMsgs.AttestationProof));
bytes32 digest = sha256(proof.attestationData);
_verifySignaturesThreshold(digest, proof.signatures);

IAttestationMsgs.PacketAttestation memory packetAttestation =
    abi.decode(proof.attestationData, (IAttestationMsgs.PacketAttestation));
require(packetAttestation.height == proofHeight, HeightMismatch(proofHeight, packetAttestation.height));
```

**Solana** ([`verify_non_membership.rs:40-49`](src/instructions/verify_non_membership.rs#L40-L49))
```rust
let proof = deserialize_membership_proof(&msg.proof)?;
let attestation = decode_packet_attestation(&proof.attestation_data)?;
require!(attestation.height == consensus_state_store.consensus_state.height, ErrorCode::HeightMismatch);
verify_attestation(client_state, &proof.attestation_data, &proof.signatures)?;
```

**Same:** Both decode, verify signatures and check height consistency. Identical logic to verify_membership proof verification.

**Difference:** Same ordering difference as previous sections: signature-then-decode (Solidity) vs decode-then-signature (Solana).

**Rationale:** See [section 2.3](#23-proof-decoding-and-signature-verification).

### 4.5 Non-membership check

**Solidity** ([`AttestationLightClient.sol:191-200`](../../../../contracts/light-clients/attestation/AttestationLightClient.sol#L191-L200))
```solidity
bytes32 pathHash = keccak256(msg_.path[0]);
require(packetAttestation.packets.length > 0, EmptyPackets());
for (uint256 i = 0; i < packetAttestation.packets.length; ++i) {
    if (packetAttestation.packets[i].path == pathHash) {
        require(packetAttestation.packets[i].commitment == bytes32(0), NotNonMember());
        return uint256(ts);
    }
}
revert NotMember();
```

**Solana** ([`verify_non_membership.rs:51-61`](src/instructions/verify_non_membership.rs#L51-L61))
```rust
require!(!attestation.packets.is_empty(), ErrorCode::EmptyAttestation);

let Hash(path_hash) = keccak256(&msg.path[0]);

let packet = attestation.packets.iter()
    .find(|p| p.path == path_hash)
    .ok_or_else(|| error!(ErrorCode::NotMember))?;

require!(packet.commitment == [0u8; 32], ErrorCode::NonZeroCommitment);
```

**Same:** Both hash the path with keccak256, search for a matching packet, then verify the commitment is zero (proving non-membership).

**Differences:**
- Same `find`-first-match vs loop-all-matches difference as membership ([section 3.5](#35-membership-check)).
- Error naming: Solidity uses `NotNonMember`, Solana uses `NonZeroCommitment`. Same semantics.

**Rationale:** See [section 3.5](#35-membership-check).

### 4.6 Return value

**Solidity** ([`AttestationLightClient.sol:197`](../../../../contracts/light-clients/attestation/AttestationLightClient.sol#L197))
```solidity
return uint256(ts);
```

**Solana** ([`verify_non_membership.rs:65-69`](src/instructions/verify_non_membership.rs#L65-L69))
```rust
let timestamp_bytes = consensus_state_store.consensus_state.timestamp.to_le_bytes();
set_return_data(&timestamp_bytes);
Ok(())
```

**Same:** Both return the consensus timestamp to the caller. Both routers use this timestamp for timeout verification in `timeout_packet` ([`ICS26Router.sol:284-288`](../../../../contracts/ICS26Router.sol#L284-L288), [`light_client_cpi.rs:36-50`](../ics26-router/src/router_cpi/light_client_cpi.rs#L36-L50)).

**Difference:** Solidity returns the timestamp as a function return value. Solana uses `set_return_data` which the router reads via `get_return_data()`.

**Rationale:** Solana CPI does not support typed return values like Solidity function calls. `set_return_data`/`get_return_data` is the standard Solana mechanism for passing data back through CPI.

---

## 5. Misbehaviour

**Solidity** ([`AttestationLightClient.sol:204-207`](../../../../contracts/light-clients/attestation/AttestationLightClient.sol#L204-L207))
```solidity
function misbehaviour(bytes calldata) external view notFrozen onlyProofSubmitter {
    revert FeatureNotSupported();
}
```

**Solana:** No corresponding instruction.

**Same:** Neither implementation supports standalone misbehaviour submission. In both cases, misbehaviour detection happens exclusively within `update_client` ([section 2.5](#25-misbehaviour-detection)).

**Difference:** Solidity exposes a standalone `misbehaviour` endpoint that always reverts. Solana omits it entirely.

**Rationale:** Solidity implements it to satisfy the `ILightClient` interface contract. Solana has no interface-conformance requirement, so a no-op instruction would waste program space.

---

## 6. Proof Encoding and Deserialization

### 6.1 Outer proof envelope

**Solidity** ([`AttestationLightClient.sol:95`](../../../../contracts/light-clients/attestation/AttestationLightClient.sol#L95), [`140`](../../../../contracts/light-clients/attestation/AttestationLightClient.sol#L140))
```solidity
IAttestationMsgs.AttestationProof memory proof = abi.decode(updateMsg, (IAttestationMsgs.AttestationProof));
```

**Solana** ([`proof.rs:10-24`](src/proof.rs#L10-L24))
```rust
pub fn deserialize_membership_proof(proof_bytes: &[u8]) -> Result<MembershipProof> {
    if proof_bytes.len() > MAX_PROOF_SIZE {
        return Err(error!(ErrorCode::InvalidProof));
    }
    MembershipProof::try_from_slice(proof_bytes).map_err(|e| {
        error!(ErrorCode::InvalidProof)
    })
}
```

**Same:** Both decode the proof envelope into a struct containing `attestation_data` and `signatures`.

**Difference:** Solidity uses ABI encoding. Solana uses Borsh encoding for the outer envelope and enforces a `MAX_PROOF_SIZE` of 64 KB. Solidity has no explicit size limit (bounded only by calldata gas costs).

**Rationale:** Borsh is the standard serialization format for Solana program instruction data. The size limit prevents excessive compute usage during deserialization.

### 6.2 Inner attestation data

**Solidity** ([`AttestationLightClient.sol:100-101`](../../../../contracts/light-clients/attestation/AttestationLightClient.sol#L100-L101))
```solidity
IAttestationMsgs.StateAttestation memory state =
    abi.decode(proof.attestationData, (IAttestationMsgs.StateAttestation));
```

**Solana** ([`abi_decode.rs:6-12`](src/abi_decode.rs#L6-L12), [`35-43`](src/abi_decode.rs#L35-L43))
```rust
mod sol_types {
    alloy_sol_types::sol!(
        "../../../../contracts/light-clients/attestation/msgs/IAttestationMsgs.sol"
    );
}

pub fn decode_state_attestation(data: &[u8]) -> Result<StateAttestation> {
    let decoded = IAttestationMsgs::StateAttestation::abi_decode(data)
        .map_err(|_| error!(ErrorCode::InvalidAttestationData))?;
    // ...
}
```

**Same:** Both ABI-decode the inner attestation data using the same `IAttestationMsgs` type definitions. The Solana decoder is generated directly from the Solidity interface file, ensuring wire-format compatibility between chains.

---

## 7. Signature Verification

### 7.1 Threshold check

**Solidity** ([`AttestationLightClient.sol:214-219`](../../../../contracts/light-clients/attestation/AttestationLightClient.sol#L214-L219))
```solidity
function _verifySignaturesThreshold(bytes32 digest, bytes[] memory signatures) private view {
    require(signatures.length > 0, EmptySignatures());
    require(
        signatures.length > clientState.minRequiredSigs - 1,
        ThresholdNotMet(signatures.length, clientState.minRequiredSigs)
    );
```

**Solana** ([`verification.rs:9-20`](src/verification.rs#L9-L20))
```rust
pub fn verify_attestation(
    client_state: &ClientState,
    attestation_data: &[u8],
    raw_signatures: &[Vec<u8>],
) -> Result<()> {
    if raw_signatures.is_empty() {
        return Err(error!(ErrorCode::EmptySignatures));
    }
    if raw_signatures.len() < client_state.min_required_sigs as usize {
        return Err(error!(ErrorCode::ThresholdNotMet));
    }
```

**Same:** Both require non-empty signatures and at least `min_required_sigs` signatures. Equivalent logic (Solidity's `> minRequiredSigs - 1` is the same as Solana's `< min_required_sigs` since the empty check runs first).

### 7.2 Digest computation

**Solidity** ([`AttestationLightClient.sol:97-98`](../../../../contracts/light-clients/attestation/AttestationLightClient.sol#L97-L98))
```solidity
// Computed once by the caller:
bytes32 digest = sha256(proof.attestationData);
_verifySignaturesThreshold(digest, proof.signatures);
```

**Solana** ([`verification.rs:22`](src/verification.rs#L22), [`crypto.rs:13-16`](src/crypto.rs#L13-L16))
```rust
let message_hash = sha256_digest(attestation_data);

for raw_sig in raw_signatures {
    let recovered_address = recover_eth_address(&message_hash, raw_sig)?;
    // ...
}
```

**Same:** Both compute SHA-256 once and reuse it for every signature. Solidity computes it at the call site and passes the digest. Solana computes it inside `verify_attestation` before the loop.

### 7.3 Signature recovery and validation

**Solidity** ([`AttestationLightClient.sol:240-248`](../../../../contracts/light-clients/attestation/AttestationLightClient.sol#L240-L248))
```solidity
function _verifySignature(bytes32 digest, bytes memory signature) private view returns (address) {
    require(signature.length == ECDSA_SIGNATURE_LENGTH, InvalidSignatureLength(signature));
    address recovered = ECDSA.recover(digest, signature);
    require(recovered != address(0), SignatureInvalid(signature));
    require(_isAttestor[recovered], UnknownSigner(recovered));
    return recovered;
}
```

**Solana** ([`crypto.rs:45-58`](src/crypto.rs#L45-L58), [`verification.rs:29-37`](src/verification.rs#L29-L37))
```rust
// In recover_eth_address:
let prepared = prepare_signature(signature)?; // checks length == 65
let pubkey = secp256k1_recover(&message_hash, prepared.recovery_id, &prepared.sig_bytes)?;
Ok(pubkey_to_eth_address(&pubkey.0))

// In verify_attestation loop:
let recovered_address = recover_eth_address(&message_hash, raw_sig)?;
if !client_state.attestor_addresses.contains(&recovered_address) {
    return Err(error!(ErrorCode::UnknownSigner));
}
```

**Same:** Both validate signature length, recover the Ethereum address from the secp256k1 signature and verify the recovered address belongs to a trusted attestor.

**Difference:** Solidity has an explicit `recovered != address(0)` guard after `ECDSA.recover`. Solana omits it. Additionally, Solidity checks attestor membership via `mapping(address => bool)` for O(1) lookup, while Solana uses `Vec::contains` for O(n).

**Rationale:** OpenZeppelin's `ECDSA.recover` can return `address(0)` for certain malformed inputs instead of reverting, so the explicit zero-address check is necessary. Solana's `secp256k1_recover` syscall returns `Err` for any invalid input — there is no code path that returns `Ok` with a zero public key (confirmed in [`solana-secp256k1-recover` source](https://github.com/anza-xyz/solana-sdk/blob/secp256k1-recover%40v2.2.1/secp256k1-recover/src/lib.rs#L396-L431)), making the check unnecessary. For attestor lookup, Solidity's `_isAttestor` mapping exists naturally from initialization ([section 1.2](#12-duplicate-attestor-check)). Solana stores attestors as a Vec in the account data. With a cap of 20 attestors, O(n) lookup is negligible compared to the secp256k1 recovery cost.

### 7.4 Duplicate detection

**Solidity** ([`AttestationLightClient.sol:222-232`](../../../../contracts/light-clients/attestation/AttestationLightClient.sol#L222-L232))
```solidity
address[] memory seen = new address[](signatures.length);
for (uint256 i = 0; i < signatures.length; ++i) {
    address recovered = _verifySignature(digest, sig);
    for (uint256 j = 0; j < i; ++j) {
        require(seen[j] != recovered, DuplicateSigner(recovered));
    }
    seen[i] = recovered;
}
```

**Solana** ([`verification.rs:25-39`](src/verification.rs#L25-L39))
```rust
let mut recovered_addresses: Vec<[u8; ETH_ADDRESS_LEN]> =
    Vec::with_capacity(raw_signatures.len());

for raw_sig in raw_signatures {
    let recovered_address = recover_eth_address(&message_hash, raw_sig)?;
    if recovered_addresses.contains(&recovered_address) {
        return Err(error!(ErrorCode::DuplicateSigner));
    }
    // ... attestor check ...
    recovered_addresses.push(recovered_address);
}
```

**Same:** Both track previously seen addresses and reject duplicates. Both use O(n^2) pairwise comparison against the seen set. Equivalent behavior.

---

## Summary Table

| # | Aspect | Solidity | Solana | Impact | Critical | Ref |
|---|--------|----------|--------|--------|----------|-----|
| 1 | Init height/timestamp validation | Allows zero | Requires > 0 | Solana stricter | No | [1.1](#11-input-validation) |
| 2 | Init `client_id` validation | None | Requires non-empty | Required for PDA seeds | No | [1.1](#11-input-validation) |
| 3 | Account versioning and reserved space | None | `AccountVersion::V1` + 256-byte reserved | Future upgrade support | No | [1.4](#14-account-versioning-and-reserved-space) |
| 4 | Access control model | `PROOF_SUBMITTER_ROLE` (OpenZeppelin) | External `access_manager` with `RELAYER_ROLE` | Architectural | No | [1.5](#15-access-control-setup) |
| 5 | Access control on verify | `onlyProofSubmitter` | None (CPI-gated) | Solana relies on router | No | [3.1](#31-access-control-and-frozen-check) |
| 6 | Client identity constraint | Implicit (one contract = one client) | `client_id` account constraint | Required for multi-client program | No | [2.2](#22-client-identity-constraint) |
| 7 | Frozen check ordering | Before role check | After role check | No semantic impact | No | [2.1](#21-access-control-and-frozen-check) |
| 8 | Signature vs decode order | Signatures first | Decode first | Solana fails faster on bad data | No | [2.3](#23-proof-decoding-and-signature-verification) |
| 9 | `update_client` height argument | None | `new_height == attestation.height` | Required for PDA model | No | [2.4](#24-state-validation) |
| 10 | `update_client` return value | `UpdateResult` enum | `Ok(())` + event | Different API surface | No | [2.5](#25-misbehaviour-detection) |
| 11 | Membership matching | Loop checks path+commitment | `find` path, then commitment | Diverges on duplicate paths (invalid case) | No | [3.5](#35-membership-check) |
| 12 | Membership error granularity | `NotMember` only | `NotMember` + `CommitmentMismatch` | Solana more specific | No | [3.5](#35-membership-check) |
| 13 | `verify_membership` return | `uint256(ts)` | `Ok(())` | Both routers ignore it | No | [3.5](#35-membership-check) |
| 14 | `verify_non_membership` return | `uint256(ts)` | `set_return_data(timestamp)` | Both routers use it for timeouts | No | [4.6](#46-return-value) |
| 15 | Standalone `misbehaviour` | Reverts `FeatureNotSupported` | Omitted | No-op in Solidity | No | [5](#5-misbehaviour) |
| 16 | Outer proof encoding | ABI | Borsh | Wire-format difference | No | [6.1](#61-outer-proof-envelope) |
| 17 | Proof size limit | None (gas-bounded) | 64 KB max | Solana explicit guard | No | [6.1](#61-outer-proof-envelope) |
| 18 | Attestor lookup complexity | `mapping` O(1) | `Vec::contains` O(n) | Negligible with ≤20 attestors | No | [7.3](#73-signature-recovery-and-validation) |
| 19 | `address(0)` recovery guard | Explicit check | Not needed (`secp256k1_recover` returns `Err`) | Library behavior | No | [7.3](#73-signature-recovery-and-validation) |
| 20 | Consensus state storage | `mapping(height => timestamp)` | PDA account per height | Architectural | No | [1.3](#13-state-storage) |

**No critical differences were found.** All differences are either architectural (required by the Solana runtime model), intentional design choices or cosmetic. The implementations are functionally equivalent for all valid inputs.
