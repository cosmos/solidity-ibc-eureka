// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { Test } from "forge-std/Test.sol";
import { stdJson } from "forge-std/StdJson.sol";

import { CometBFTClient } from "../../contracts/light-clients/cometbft/CometBFTClient.sol";
import { ICometBFTClientErrors } from "../../contracts/light-clients/cometbft/errors/ICometBFTClientErrors.sol";
import { ICometBFTMsgs } from "../../contracts/light-clients/cometbft/msgs/ICometBFTMsgs.sol";
import { CometBFTECDSA } from "../../contracts/light-clients/cometbft/utils/CometBFTECDSA.sol";
import { CometBFTICS23 } from "../../contracts/light-clients/cometbft/utils/CometBFTICS23.sol";
import { CometBFTMerkle } from "../../contracts/light-clients/cometbft/utils/CometBFTMerkle.sol";
import { CometBFTProto } from "../../contracts/light-clients/cometbft/utils/CometBFTProto.sol";
import { IICS02ClientMsgs } from "../../contracts/msgs/IICS02ClientMsgs.sol";
import { ILightClientMsgs } from "../../contracts/msgs/ILightClientMsgs.sol";

contract CometBFTGasBenchmark {
    uint256 private constant SECP256K1_P = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F;

    struct EthPubKeyValidator {
        bytes pubKey;
        uint64 votingPower;
    }

    function compressedWitnessPath(ICometBFTMsgs.Validator[] memory validators)
        external
        pure
        returns (bytes32 validatorSetHash, bytes32 identityHash)
    {
        bytes[] memory leaves = new bytes[](validators.length);
        for (uint256 i = 0; i < validators.length; ++i) {
            leaves[i] = CometBFTProto.encodeSimpleValidator(validators[i]);
            identityHash = keccak256(abi.encodePacked(identityHash, _validatorAddress(validators[i])));
        }
        validatorSetHash = CometBFTMerkle.hashFromByteSlices(leaves);
    }

    function uncompressedPubKeyPath(EthPubKeyValidator[] memory validators)
        external
        pure
        returns (bytes32 validatorSetHash, bytes32 identityHash)
    {
        bytes[] memory leaves = new bytes[](validators.length);
        for (uint256 i = 0; i < validators.length; ++i) {
            bytes memory pubKey = validators[i].pubKey;
            require(pubKey.length == 64, "bad eth pubkey");

            leaves[i] = _encodeSimpleValidatorWithEthPubKey(pubKey, validators[i].votingPower);
            address signer = address(uint160(uint256(keccak256(pubKey))));
            identityHash = keccak256(abi.encodePacked(identityHash, signer));
        }
        validatorSetHash = CometBFTMerkle.hashFromByteSlices(leaves);
    }

    function _encodeSimpleValidatorWithEthPubKey(
        bytes memory pubKey,
        uint64 votingPower
    )
        private
        pure
        returns (bytes memory)
    {
        bytes memory pubKeyField = abi.encodePacked(bytes1(0x2a), CometBFTProto.encodeVarint(pubKey.length), pubKey);
        return abi.encodePacked(
            bytes1(0x0a),
            CometBFTProto.encodeVarint(pubKeyField.length),
            pubKeyField,
            votingPower == 0 ? bytes("") : abi.encodePacked(bytes1(0x10), CometBFTProto.encodeVarint(votingPower))
        );
    }

    function _validatorAddress(ICometBFTMsgs.Validator memory validator) private pure returns (address) {
        bytes memory pubKey = validator.pubKey;
        require(pubKey.length == 33, "bad compressed pubkey");
        uint8 prefix = uint8(pubKey[0]);
        require(prefix == 0x02 || prefix == 0x03, "bad compressed pubkey prefix");

        uint256 x;
        assembly ("memory-safe") {
            x := mload(add(pubKey, 33))
        }
        uint256 y = uint256(validator.y);
        require(x < SECP256K1_P && y < SECP256K1_P, "bad coordinates");
        require(
            mulmod(y, y, SECP256K1_P) == addmod(mulmod(mulmod(x, x, SECP256K1_P), x, SECP256K1_P), 7, SECP256K1_P),
            "off curve"
        );
        require(uint8(y & 1) == prefix - 2, "bad parity");
        return address(uint160(uint256(keccak256(abi.encodePacked(x, y)))));
    }
}

contract CometBFTICS23Harness {
    function decodeMembershipProof(
        bytes calldata proof,
        bytes[] calldata path,
        bytes calldata value
    )
        external
        pure
        returns (
            uint256 proofCount,
            bytes32 leafKeyHash,
            bytes32 leafValueHash,
            uint8 leafHashOp,
            bytes32 leafPrefixHash,
            uint256 leafInnerOpCount,
            bytes32 parentKeyHash
        )
    {
        ICometBFTMsgs.ICS23Proof memory decoded = CometBFTICS23.decodeMembershipProof(proof, path, value);
        ICometBFTMsgs.ICS23ExistenceProof memory leaf = decoded.proofs[0].existence;
        ICometBFTMsgs.ICS23ExistenceProof memory parent = decoded.proofs[1].existence;
        return (
            decoded.proofs.length,
            keccak256(leaf.key),
            keccak256(leaf.value),
            leaf.leaf.hash,
            keccak256(leaf.leaf.prefix),
            leaf.path.length,
            keccak256(parent.key)
        );
    }

    function decodeNonMembershipProof(
        bytes calldata proof,
        bytes[] calldata path
    )
        external
        pure
        returns (uint256 proofCount, bytes32 missingKeyHash, bytes32 rightNeighborKeyHash, bytes32 parentKeyHash)
    {
        ICometBFTMsgs.ICS23Proof memory decoded = CometBFTICS23.decodeNonMembershipProof(proof, path);
        ICometBFTMsgs.ICS23NonExistenceProof memory leaf = decoded.proofs[0].nonExistence;
        ICometBFTMsgs.ICS23ExistenceProof memory parent = decoded.proofs[1].existence;
        return (decoded.proofs.length, keccak256(leaf.key), keccak256(leaf.right.proof.key), keccak256(parent.key));
    }
}

contract CometBFTClientTest is Test {
    using stdJson for string;

    string private fixtureJson;
    string private ics23FixtureJson;
    string private realMembershipFixtureJson;
    string private realNonMembershipFixtureJson;
    string private misbehaviourFixtureJson;
    CometBFTClient private client;
    CometBFTGasBenchmark private gasBenchmark;
    CometBFTICS23Harness private ics23Harness;

    function setUp() public {
        gasBenchmark = new CometBFTGasBenchmark();
        ics23Harness = new CometBFTICS23Harness();
        ics23FixtureJson = vm.readFile(_fixturePath("native_ics23_parser_fixture.json"));
        realMembershipFixtureJson = vm.readFile(_fixturePath("native_ics23_membership_fixture.json"));
        realNonMembershipFixtureJson = vm.readFile(_fixturePath("native_ics23_non_membership_fixture.json"));
        misbehaviourFixtureJson = vm.readFile(_fixturePath("native_misbehaviour_fixture.json"));
        _loadFixture("native_update_fixture.json");
    }

    function _loadFixture(string memory fileName) private {
        fixtureJson = vm.readFile(_fixturePath(fileName));
        vm.warp(fixtureJson.readUint(".proofNow"));

        client = new CometBFTClient(_clientState(), _trustedConsensusState(), _trustedValidators(), address(this));
    }

    function test_fixtureVectorsMatchSolidityCodecs() public view {
        _assertFixtureVectorsMatch();
    }

    function test_validAdjacentUpdateClient() public {
        _assertValidAdjacentUpdateClient();
    }

    function test_twentyValidatorFixtureVectorsMatchSolidityCodecs() public {
        _loadFixture("native_update_20_validators_fixture.json");
        assertEq(_validators().length, 20);

        _assertFixtureVectorsMatch();
    }

    function test_twentyValidatorValidAdjacentUpdateClient() public {
        _loadFixture("native_update_20_validators_fixture.json");
        assertEq(_validators().length, 20);

        _assertValidAdjacentUpdateClient();
    }

    function test_validSkippingUpdateClient() public {
        _loadFixture("native_skipping_update_fixture.json");
        _assertValidSkippingFixtureShape();
        assertNotEq(_header().validatorsHash, _header().nextValidatorsHash);
        assertNotEq(CometBFTProto.validatorSetHash(_validators()), CometBFTProto.validatorSetHash(_nextValidators()));

        _assertValidAdjacentUpdateClient();
    }

    function test_validSkippingUpdateStoresNextValidatorSetForFutureTrust() public {
        _loadFixture("native_skipping_update_fixture.json");
        _assertValidSkippingFixtureShape();
        _assertValidAdjacentUpdateClient();

        fixtureJson = vm.readFile(_fixturePath("native_skipping_next_update_fixture.json"));
        vm.warp(fixtureJson.readUint(".proofNow"));
        _assertValidSkippingFixtureShape();

        _assertValidAdjacentUpdateClient();
    }

    function test_emptyInitialTrustedValidatorSetCannotUpdate() public {
        CometBFTClient proofOnlyClient =
            new CometBFTClient(_clientState(), _trustedConsensusState(), _emptyValidators(), address(this));

        vm.expectRevert(
            abi.encodeWithSelector(
                ICometBFTClientErrors.TrustedValidatorSetNotFound.selector,
                _trustedHeight().revisionNumber,
                _trustedHeight().revisionHeight
            )
        );
        proofOnlyClient.updateClient(abi.encode(_updateMsg()));
    }

    function test_twentyValidatorValidSkippingUpdateClient() public {
        _loadFixture("native_skipping_update_20_validators_fixture.json");
        assertEq(_validators().length, 20);
        _assertValidSkippingFixtureShape();

        _assertValidAdjacentUpdateClient();
    }

    function test_adjacentUpdateMessageSize() public {
        emit log_named_uint("adjacent update calldata bytes", abi.encode(_updateMsg()).length);
    }

    function test_twentyValidatorAdjacentUpdateMessageSize() public {
        _loadFixture("native_update_20_validators_fixture.json");
        emit log_named_uint("20-validator adjacent update calldata bytes", abi.encode(_updateMsg()).length);
    }

    function test_skippingUpdateMessageSize() public {
        _loadFixture("native_skipping_update_fixture.json");
        emit log_named_uint("skipping update calldata bytes", abi.encode(_updateMsg()).length);
    }

    function test_twentyValidatorSkippingUpdateMessageSize() public {
        _loadFixture("native_skipping_update_20_validators_fixture.json");
        emit log_named_uint("20-validator skipping update calldata bytes", abi.encode(_updateMsg()).length);
    }

    function test_skippingUpdateRejectsInsufficientTrustedOverlap() public {
        _loadFixture("native_skipping_insufficient_trusted_overlap_fixture.json");
        assertGt(_header().height, _trustedHeight().revisionHeight + 1);
        assertNotEq(_header().validatorsHash, _trustedConsensusState().nextValidatorsHash);
        assertLe(
            fixtureJson.readUint(".expected.trustedSignedVotingPower"),
            fixtureJson.readUint(".expected.trustedVotingPowerNeeded")
        );
        assertGt(
            fixtureJson.readUint(".expected.newSignedVotingPower"),
            fixtureJson.readUint(".expected.newVotingPowerNeeded")
        );
        _assertFixtureVectorsMatch();

        ICometBFTMsgs.MsgUpdateClient memory msg_ = _updateMsg();
        vm.expectRevert(
            abi.encodeWithSelector(
                ICometBFTClientErrors.NotEnoughTrustedVotingPower.selector,
                fixtureJson.readUint(".expected.trustedSignedVotingPower"),
                fixtureJson.readUint(".expected.trustedVotingPowerNeeded")
            )
        );
        client.updateClient(abi.encode(msg_));
    }

    function test_skippingUpdateRejectsWrongNextValidatorSet() public {
        _loadFixture("native_skipping_update_fixture.json");
        ICometBFTMsgs.MsgUpdateClient memory msg_ = _updateMsg();
        msg_.nextValidators[0].votingPower += 1;

        vm.expectRevert(
            abi.encodeWithSelector(
                ICometBFTClientErrors.NextValidatorSetHashMismatch.selector,
                _header().nextValidatorsHash,
                CometBFTProto.validatorSetHash(msg_.nextValidators)
            )
        );
        client.updateClient(abi.encode(msg_));
    }

    function test_validAdjacentUpdateClientReplayIsNoOp() public {
        ICometBFTMsgs.MsgUpdateClient memory msg_ = _updateMsg();

        ILightClientMsgs.UpdateResult result = client.updateClient(abi.encode(msg_));
        assertEq(uint256(result), uint256(ILightClientMsgs.UpdateResult.Update));

        result = client.updateClient(abi.encode(msg_));
        assertEq(uint256(result), uint256(ILightClientMsgs.UpdateResult.NoOp));
    }

    function test_misbehaviourFixtureVectorsMatchSolidityCodecs() public view {
        assertEq(
            CometBFTProto.headerHash(_misbehaviourHeader(".doubleSign")),
            misbehaviourFixtureJson.readBytes32(".expected.doubleSignHeaderHash")
        );
        assertEq(
            CometBFTProto.headerHash(_misbehaviourHeader(".timeViolation")),
            misbehaviourFixtureJson.readBytes32(".expected.timeViolationHeaderHash")
        );
        assertEq(
            CometBFTProto.validatorSetHash(_misbehaviourValidators()), _misbehaviourHeader(".doubleSign").validatorsHash
        );
        assertEq(
            CometBFTProto.validatorSetHash(_misbehaviourValidators()),
            _misbehaviourHeader(".timeViolation").validatorsHash
        );
    }

    function test_initialConsensusStateStoredByFullHeight() public view {
        IICS02ClientMsgs.Height memory trustedHeight = _trustedHeight();
        ICometBFTMsgs.ConsensusState memory trustedConsensusState = _trustedConsensusState();

        assertEq(client.getConsensusStateHash(trustedHeight), keccak256(abi.encode(trustedConsensusState)));
        assertEq(
            client.getConsensusStateHash(trustedHeight.revisionHeight), keccak256(abi.encode(trustedConsensusState))
        );
        _assertConsensusStateEq(client.getConsensusState(trustedHeight), trustedConsensusState);
    }

    function test_consensusStateStorageSupportsNonzeroRevision() public {
        ICometBFTMsgs.ClientState memory initialClientState = _clientState();
        initialClientState.latestHeight.revisionNumber = 7;
        CometBFTClient revisionClient =
            new CometBFTClient(initialClientState, _trustedConsensusState(), _trustedValidators(), address(this));

        IICS02ClientMsgs.Height memory trustedHeight = _trustedHeight();
        trustedHeight.revisionNumber = 7;
        _assertConsensusStateEq(revisionClient.getConsensusState(trustedHeight), _trustedConsensusState());
        assertEq(revisionClient.getConsensusStateHash(trustedHeight), keccak256(abi.encode(_trustedConsensusState())));
        assertEq(
            revisionClient.getConsensusStateHash(trustedHeight.revisionHeight),
            keccak256(abi.encode(_trustedConsensusState()))
        );

        ICometBFTMsgs.MsgUpdateClient memory msg_ = _updateMsg();
        msg_.trustedHeight.revisionNumber = 7;
        ILightClientMsgs.UpdateResult result = revisionClient.updateClient(abi.encode(msg_));
        assertEq(uint256(result), uint256(ILightClientMsgs.UpdateResult.Update));

        ICometBFTMsgs.ConsensusState memory expectedConsensusState = _newConsensusState(msg_.header);
        IICS02ClientMsgs.Height memory expectedHeight =
            IICS02ClientMsgs.Height({ revisionNumber: 7, revisionHeight: msg_.header.height });
        _assertConsensusStateEq(revisionClient.getConsensusState(expectedHeight), expectedConsensusState);
        assertEq(revisionClient.getConsensusStateHash(expectedHeight), keccak256(abi.encode(expectedConsensusState)));
        assertEq(
            revisionClient.getConsensusStateHash(expectedHeight.revisionHeight),
            keccak256(abi.encode(expectedConsensusState))
        );
    }

    function test_wrongTrustedRevisionFails() public {
        ICometBFTMsgs.MsgUpdateClient memory msg_ = _updateMsg();
        msg_.trustedHeight.revisionNumber += 1;

        vm.expectRevert(
            abi.encodeWithSelector(
                ICometBFTClientErrors.RevisionNumberMismatch.selector,
                _trustedHeight().revisionNumber,
                msg_.trustedHeight.revisionNumber
            )
        );
        client.updateClient(abi.encode(msg_));
    }

    function testFuzz_wrongTrustedRevisionFails(uint64 wrongRevision) public {
        vm.assume(wrongRevision != _trustedHeight().revisionNumber);

        ICometBFTMsgs.MsgUpdateClient memory msg_ = _updateMsg();
        msg_.trustedHeight.revisionNumber = wrongRevision;

        vm.expectRevert(
            abi.encodeWithSelector(
                ICometBFTClientErrors.RevisionNumberMismatch.selector,
                _trustedHeight().revisionNumber,
                msg_.trustedHeight.revisionNumber
            )
        );
        client.updateClient(abi.encode(msg_));
    }

    function test_fullHeightConsensusStateKeyIncludesRevision() public {
        ICometBFTMsgs.MsgUpdateClient memory msg_ = _updateMsg();
        client.updateClient(abi.encode(msg_));

        IICS02ClientMsgs.Height memory wrongRevision = IICS02ClientMsgs.Height({
            revisionNumber: _trustedHeight().revisionNumber + 1, revisionHeight: msg_.header.height
        });
        vm.expectRevert(
            abi.encodeWithSelector(
                ICometBFTClientErrors.ConsensusStateNotFound.selector,
                wrongRevision.revisionNumber,
                wrongRevision.revisionHeight
            )
        );
        client.getConsensusStateHash(wrongRevision);

        vm.expectRevert(
            abi.encodeWithSelector(
                ICometBFTClientErrors.ConsensusStateNotFound.selector,
                wrongRevision.revisionNumber,
                wrongRevision.revisionHeight
            )
        );
        client.getConsensusState(wrongRevision);
    }

    function test_unknownConsensusStateHeightFailsInCurrentRevision() public {
        IICS02ClientMsgs.Height memory unknownHeight = IICS02ClientMsgs.Height({
            revisionNumber: _trustedHeight().revisionNumber, revisionHeight: _trustedHeight().revisionHeight + 99
        });

        vm.expectRevert(
            abi.encodeWithSelector(
                ICometBFTClientErrors.ConsensusStateNotFound.selector,
                unknownHeight.revisionNumber,
                unknownHeight.revisionHeight
            )
        );
        client.getConsensusStateHash(unknownHeight);

        vm.expectRevert(
            abi.encodeWithSelector(
                ICometBFTClientErrors.ConsensusStateNotFound.selector,
                unknownHeight.revisionNumber,
                unknownHeight.revisionHeight
            )
        );
        client.getConsensusStateHash(unknownHeight.revisionHeight);

        vm.expectRevert(
            abi.encodeWithSelector(
                ICometBFTClientErrors.ConsensusStateNotFound.selector,
                unknownHeight.revisionNumber,
                unknownHeight.revisionHeight
            )
        );
        client.getConsensusState(unknownHeight);
    }

    function test_invalidHeaderTimestampNanosFails() public {
        ICometBFTMsgs.MsgUpdateClient memory msg_ = _updateMsg();
        msg_.header.timeNanos = 1_000_000_000;

        vm.expectRevert(
            abi.encodeWithSelector(ICometBFTClientErrors.InvalidHeaderTimestampNanos.selector, msg_.header.timeNanos)
        );
        client.updateClient(abi.encode(msg_));
    }

    function testFuzz_invalidHeaderTimestampNanosFails(uint32 timeNanos) public {
        timeNanos = uint32(bound(uint256(timeNanos), 1_000_000_000, type(uint32).max));

        ICometBFTMsgs.MsgUpdateClient memory msg_ = _updateMsg();
        msg_.header.timeNanos = timeNanos;

        vm.expectRevert(
            abi.encodeWithSelector(ICometBFTClientErrors.InvalidHeaderTimestampNanos.selector, msg_.header.timeNanos)
        );
        client.updateClient(abi.encode(msg_));
    }

    function test_invalidCommitTimestampNanosFails() public {
        ICometBFTMsgs.MsgUpdateClient memory msg_ = _updateMsg();
        msg_.commit.signatures[0].timestampNanos = 1_000_000_000;

        vm.expectRevert(
            abi.encodeWithSelector(
                ICometBFTClientErrors.InvalidCommitTimestampNanos.selector, 0, msg_.commit.signatures[0].timestampNanos
            )
        );
        client.updateClient(abi.encode(msg_));
    }

    function testFuzz_invalidCommitTimestampNanosFails(uint32 timeNanos) public {
        timeNanos = uint32(bound(uint256(timeNanos), 1_000_000_000, type(uint32).max));

        ICometBFTMsgs.MsgUpdateClient memory msg_ = _updateMsg();
        msg_.commit.signatures[0].timestampNanos = timeNanos;

        vm.expectRevert(
            abi.encodeWithSelector(
                ICometBFTClientErrors.InvalidCommitTimestampNanos.selector, 0, msg_.commit.signatures[0].timestampNanos
            )
        );
        client.updateClient(abi.encode(msg_));
    }

    function test_invalidInitialClientStatePeriodsFail() public {
        ICometBFTMsgs.ClientState memory state = _clientState();
        state.trustingPeriod = state.unbondingPeriod;

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidClientState.selector));
        new CometBFTClient(state, _trustedConsensusState(), _trustedValidators(), address(this));
    }

    function test_invalidSignatureVFails() public {
        ICometBFTMsgs.MsgUpdateClient memory msg_ = _updateMsg();
        msg_.commit.signatures[0].signature[64] = bytes1(uint8(2));

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidSignatureV.selector, uint8(2)));
        client.updateClient(abi.encode(msg_));
    }

    function test_insufficientVotingPowerFails() public {
        ICometBFTMsgs.MsgUpdateClient memory msg_ = _updateMsg();
        _makeAbsent(msg_.commit.signatures[0]);
        _makeAbsent(msg_.commit.signatures[1]);

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.NotEnoughVotingPower.selector, 10, 40));
        client.updateClient(abi.encode(msg_));
    }

    function test_malformedTrailingAbsentSignatureFailsBeforeQuorumEarlyReturn() public {
        ICometBFTMsgs.MsgUpdateClient memory msg_ = _updateMsg();
        msg_.commit.signatures[2].blockIdFlag = 1;

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidAbsentCommitSignature.selector, 2));
        client.updateClient(abi.encode(msg_));
    }

    function test_nilVoteSignatureIsVerifiedButNotCounted() public {
        ICometBFTMsgs.MsgUpdateClient memory msg_ = _updateMsg();
        msg_.commit.signatures[0].blockIdFlag = 3;

        vm.expectRevert();
        client.updateClient(abi.encode(msg_));
    }

    function test_nilVoteMissingSignatureFails() public {
        ICometBFTMsgs.MsgUpdateClient memory msg_ = _updateMsg();
        msg_.commit.signatures[0].blockIdFlag = 3;
        msg_.commit.signatures[0].signature = "";

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidSignatureLength.selector, 0));
        client.updateClient(abi.encode(msg_));
    }

    function test_validatorSetMustUseCometBFTOrdering() public {
        ICometBFTMsgs.MsgUpdateClient memory msg_ = _updateMsg();
        ICometBFTMsgs.Validator memory first = msg_.validators[0];
        msg_.validators[0] = msg_.validators[1];
        msg_.validators[1] = first;

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidValidatorOrdering.selector, 1));
        client.updateClient(abi.encode(msg_));
    }

    function test_verifyMembershipSuccess() public {
        CometBFTClient membershipClient =
            _membershipFixtureClient(false, ics23FixtureJson.readBytes32(".membership.root"));

        uint256 timestamp = membershipClient.verifyMembership(_membershipVerifyMsg());

        assertEq(timestamp, uint256(_trustedConsensusState().timestamp / 1e9));
    }

    function test_verifyMembershipRealCometBFTFixtureSuccess() public {
        CometBFTClient membershipClient = _realMembershipFixtureClient(false);

        uint256 timestamp = membershipClient.verifyMembership(_realMembershipVerifyMsg());

        assertEq(timestamp, realMembershipFixtureJson.readUint(".membership.timestamp") / 1e9);
    }

    function test_verifyMembershipRealCometBFTFixtureRejectsWrongValue() public {
        CometBFTClient membershipClient = _realMembershipFixtureClient(false);
        ILightClientMsgs.MsgVerifyMembership memory msg_ = _realMembershipVerifyMsg();
        msg_.value = "wrong";

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidICS23Proof.selector));
        membershipClient.verifyMembership(msg_);
    }

    function testFuzz_verifyMembershipRejectsUnknownProofHeight(uint64 revisionHeight) public {
        IICS02ClientMsgs.Height memory knownHeight = _realMembershipHeight();
        vm.assume(revisionHeight != knownHeight.revisionHeight);

        CometBFTClient membershipClient = _realMembershipFixtureClient(false);
        ILightClientMsgs.MsgVerifyMembership memory msg_ = _realMembershipVerifyMsg();
        msg_.proofHeight.revisionHeight = revisionHeight;

        vm.expectRevert(
            abi.encodeWithSelector(
                ICometBFTClientErrors.ConsensusStateNotFound.selector,
                knownHeight.revisionNumber,
                msg_.proofHeight.revisionHeight
            )
        );
        membershipClient.verifyMembership(msg_);
    }

    function test_verifyMembershipRejectsWrongConsensusRoot() public {
        CometBFTClient membershipClient = _membershipFixtureClient(false, bytes32(uint256(1)));

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidICS23Proof.selector));
        membershipClient.verifyMembership(_membershipVerifyMsg());
    }

    function test_verifyMembershipRejectsUnknownConsensusState() public {
        CometBFTClient membershipClient =
            _membershipFixtureClient(false, ics23FixtureJson.readBytes32(".membership.root"));
        ILightClientMsgs.MsgVerifyMembership memory msg_ = _membershipVerifyMsg();
        msg_.proofHeight.revisionHeight += 1;

        vm.expectRevert(
            abi.encodeWithSelector(
                ICometBFTClientErrors.ConsensusStateNotFound.selector,
                msg_.proofHeight.revisionNumber,
                msg_.proofHeight.revisionHeight
            )
        );
        membershipClient.verifyMembership(msg_);
    }

    function test_verifyMembershipRejectsFrozenClient() public {
        CometBFTClient membershipClient =
            _membershipFixtureClient(true, ics23FixtureJson.readBytes32(".membership.root"));

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.FrozenClientState.selector));
        membershipClient.verifyMembership(_membershipVerifyMsg());
    }

    function test_verifyMembershipRejectsInvalidLeafSpec() public {
        ICometBFTMsgs.ICS23Proof memory proof = _membershipProofFromFixture();
        proof.proofs[0].existence.leaf.hash = 0;

        _expectMembershipProofRevert(proof);
    }

    function test_verifyMembershipRejectsInvalidIavlLeafPrefix() public {
        ICometBFTMsgs.ICS23Proof memory proof = _membershipProofFromFixture();
        proof.proofs[0].existence.leaf.prefix = hex"00";

        _expectMembershipProofRevert(proof);
    }

    function test_verifyMembershipRejectsInvalidIavlInnerPrefix() public {
        ICometBFTMsgs.ICS23Proof memory proof = _membershipProofFromFixture();
        proof.proofs[0].existence.path[0].prefix = hex"00000020";

        _expectMembershipProofRevert(proof);
    }

    function test_verifyMembershipRejectsInvalidIavlInnerSuffix() public {
        ICometBFTMsgs.ICS23Proof memory proof = _membershipProofFromFixture();
        proof.proofs[0].existence.path[0].suffix = hex"01";

        _expectMembershipProofRevert(proof);
    }

    function test_verifyMembershipRejectsInvalidTendermintParentSpec() public {
        ICometBFTMsgs.ICS23Proof memory proof = _membershipProofFromFixture();
        proof.proofs[1].existence.leaf.prehashValue = 0;

        _expectMembershipProofRevert(proof);
    }

    function test_verifyMembershipRejectsRemovedInnerOp() public {
        ICometBFTMsgs.ICS23Proof memory proof = _membershipProofFromFixture();
        proof.proofs[0].existence.path = new ICometBFTMsgs.ICS23InnerOp[](0);

        _expectMembershipProofRevert(proof);
    }

    function test_verifyMembershipRejectsAlteredSiblingBytes() public {
        ICometBFTMsgs.ICS23Proof memory proof = _membershipProofFromFixture();
        proof.proofs[1].existence.path[0].suffix = abi.encodePacked(bytes32(uint256(1)));

        _expectMembershipProofRevert(proof);
    }

    function _expectMembershipProofRevert(ICometBFTMsgs.ICS23Proof memory proof) private {
        CometBFTClient membershipClient =
            _membershipFixtureClient(false, ics23FixtureJson.readBytes32(".membership.root"));
        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidICS23Proof.selector));
        membershipClient.verifyMembership(_membershipVerifyMsg(proof));
    }

    function test_verifyNonMembershipRealCometBFTFixtureSuccess() public {
        CometBFTClient nonMembershipClient = _realNonMembershipFixtureClient(false);

        uint256 timestamp = nonMembershipClient.verifyNonMembership(_realNonMembershipVerifyMsg());

        assertEq(timestamp, realNonMembershipFixtureJson.readUint(".nonMembership.timestamp") / 1e9);
    }

    function test_verifyNonMembershipRealCometBFTFixtureRejectsWrongConsensusRoot() public {
        CometBFTClient nonMembershipClient = _realNonMembershipFixtureClient(false, bytes32(uint256(1)));

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidICS23Proof.selector));
        nonMembershipClient.verifyNonMembership(_realNonMembershipVerifyMsg());
    }

    function test_verifyNonMembershipRealCometBFTFixtureRejectsWrongPath() public {
        CometBFTClient nonMembershipClient = _realNonMembershipFixtureClient(false);
        ILightClientMsgs.MsgVerifyNonMembership memory msg_ = _realNonMembershipVerifyMsg();
        msg_.path[1] = "clients/07-tendermint-001/wrong";

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidICS23Proof.selector));
        nonMembershipClient.verifyNonMembership(msg_);
    }

    function test_verifyNonMembershipRealCometBFTFixtureRejectsAlteredRightNeighbor() public {
        ILightClientMsgs.MsgVerifyNonMembership memory msg_ = _realNonMembershipVerifyMsg();
        ICometBFTMsgs.ICS23Proof memory proof = abi.decode(msg_.proof, (ICometBFTMsgs.ICS23Proof));
        proof.proofs[0].nonExistence.right.proof.path[0].suffix = abi.encodePacked(bytes32(uint256(1)));

        _expectRealNonMembershipProofRevert(proof);
    }

    function test_verifyNonMembershipRealCometBFTFixtureRejectsAlteredLeftNeighbor() public {
        ICometBFTMsgs.ICS23Proof memory proof = _realNonMembershipProofFromFixture();
        assertTrue(proof.proofs[0].nonExistence.left.exists);
        proof.proofs[0].nonExistence.left.proof.value = "altered-left-neighbor-value";

        _expectRealNonMembershipProofRevert(proof);
    }

    function test_verifyNonMembershipRealCometBFTFixtureRejectsBadLeftNeighborOrder() public {
        ICometBFTMsgs.ICS23Proof memory proof = _realNonMembershipProofFromFixture();
        proof.proofs[0].nonExistence.left.proof.key = proof.proofs[0].nonExistence.key;

        _expectRealNonMembershipProofRevert(proof);
    }

    function test_verifyNonMembershipRealCometBFTFixtureRejectsBadRightNeighborOrder() public {
        ICometBFTMsgs.ICS23Proof memory proof = _realNonMembershipProofFromFixture();
        proof.proofs[0].nonExistence.right.proof.key = proof.proofs[0].nonExistence.key;

        _expectRealNonMembershipProofRevert(proof);
    }

    function test_verifyNonMembershipRealCometBFTFixtureRejectsRemovedLeftNeighbor() public {
        ICometBFTMsgs.ICS23Proof memory proof = _realNonMembershipProofFromFixture();
        assertTrue(proof.proofs[0].nonExistence.left.exists);
        delete proof.proofs[0].nonExistence.left;

        _expectRealNonMembershipProofRevert(proof);
    }

    function test_verifyNonMembershipRealCometBFTFixtureRejectsRemovedRightNeighbor() public {
        ICometBFTMsgs.ICS23Proof memory proof = _realNonMembershipProofFromFixture();
        assertTrue(proof.proofs[0].nonExistence.right.exists);
        delete proof.proofs[0].nonExistence.right;

        _expectRealNonMembershipProofRevert(proof);
    }

    function test_verifyNonMembershipRealCometBFTFixtureRejectsInvalidNeighborLeafSpec() public {
        ICometBFTMsgs.ICS23Proof memory proof = _realNonMembershipProofFromFixture();
        proof.proofs[0].nonExistence.right.proof.leaf.hash = 0;

        _expectRealNonMembershipProofRevert(proof);
    }

    function test_verifyNonMembershipRealCometBFTFixtureRejectsAlteredParentProof() public {
        ICometBFTMsgs.ICS23Proof memory proof = _realNonMembershipProofFromFixture();
        proof.proofs[1].existence.path[0].suffix = abi.encodePacked(bytes32(uint256(1)));

        _expectRealNonMembershipProofRevert(proof);
    }

    function _expectRealNonMembershipProofRevert(ICometBFTMsgs.ICS23Proof memory proof) private {
        CometBFTClient nonMembershipClient = _realNonMembershipFixtureClient(false);
        ILightClientMsgs.MsgVerifyNonMembership memory msg_ = _realNonMembershipVerifyMsg();
        msg_.proof = abi.encode(proof);

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidICS23Proof.selector));
        nonMembershipClient.verifyNonMembership(msg_);
    }

    function test_verifyNonMembershipRejectsFrozenClient() public {
        CometBFTClient nonMembershipClient = _realNonMembershipFixtureClient(true);

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.FrozenClientState.selector));
        nonMembershipClient.verifyNonMembership(_realNonMembershipVerifyMsg());
    }

    function test_decodeNativeICS23MembershipProof() public view {
        bytes[] memory path = ics23FixtureJson.readBytesArray(".membership.path");
        bytes memory value = ics23FixtureJson.readBytes(".membership.value");
        bytes memory proof = ics23FixtureJson.readBytes(".membership.proof");

        (
            uint256 proofCount,
            bytes32 leafKeyHash,
            bytes32 leafValueHash,
            uint8 leafHashOp,
            bytes32 leafPrefixHash,
            uint256 leafInnerOpCount,
            bytes32 parentKeyHash
        ) = ics23Harness.decodeMembershipProof(proof, path, value);

        assertEq(proofCount, path.length);
        assertEq(leafKeyHash, keccak256(path[1]));
        assertEq(leafValueHash, keccak256(value));
        assertEq(leafHashOp, 1);
        assertEq(leafPrefixHash, keccak256(hex"000000"));
        assertEq(leafInnerOpCount, 1);
        assertEq(parentKeyHash, keccak256(path[0]));
    }

    function test_decodeNativeICS23NonMembershipProof() public view {
        bytes[] memory path = ics23FixtureJson.readBytesArray(".nonMembership.path");
        bytes memory proof = ics23FixtureJson.readBytes(".nonMembership.proof");

        (uint256 proofCount, bytes32 missingKeyHash, bytes32 rightNeighborKeyHash, bytes32 parentKeyHash) =
            ics23Harness.decodeNonMembershipProof(proof, path);

        assertEq(proofCount, path.length);
        assertEq(missingKeyHash, keccak256(path[1]));
        assertEq(rightNeighborKeyHash, keccak256(bytes("clients/07-tendermint-0/next")));
        assertEq(parentKeyHash, keccak256(path[0]));
    }

    function test_decodeNativeICS23MembershipRejectsEmptyValue() public {
        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.EmptyMembershipValue.selector));
        ics23Harness.decodeMembershipProof(abi.encode(_membershipICS23Proof()), _proofPath(), "");
    }

    function test_decodeNativeICS23ProofRejectsEmptyPath() public {
        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidICS23Proof.selector));
        ics23Harness.decodeMembershipProof(abi.encode(_membershipICS23Proof()), new bytes[](0), "value");
    }

    function test_decodeNativeICS23ProofRejectsPathProofLengthMismatch() public {
        bytes[] memory path = new bytes[](1);
        path[0] = "ibc";

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidICS23Proof.selector));
        ics23Harness.decodeMembershipProof(abi.encode(_membershipICS23Proof()), path, "value");
    }

    function test_decodeNativeICS23MembershipRejectsNonExistenceProof() public {
        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidICS23Proof.selector));
        ics23Harness.decodeMembershipProof(abi.encode(_nonMembershipICS23Proof()), _proofPath(), "value");
    }

    function test_decodeNativeICS23MembershipRejectsWrongValue() public {
        ICometBFTMsgs.ICS23Proof memory proof = _membershipICS23Proof();
        proof.proofs[0].existence.value = "wrong";

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidICS23Proof.selector));
        ics23Harness.decodeMembershipProof(abi.encode(proof), _proofPath(), "value");
    }

    function test_decodeNativeICS23MembershipRejectsWrongLeafKey() public {
        ICometBFTMsgs.ICS23Proof memory proof = _membershipICS23Proof();
        proof.proofs[0].existence.key = "clients/07-tendermint-0/wrong";

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidICS23Proof.selector));
        ics23Harness.decodeMembershipProof(abi.encode(proof), _proofPath(), "value");
    }

    function test_decodeNativeICS23MembershipRejectsWrongParentKey() public {
        ICometBFTMsgs.ICS23Proof memory proof = _membershipICS23Proof();
        proof.proofs[1].existence.key = "wrong-store";

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidICS23Proof.selector));
        ics23Harness.decodeMembershipProof(abi.encode(proof), _proofPath(), "value");
    }

    function test_decodeNativeICS23NonMembershipRejectsExistenceFirstProof() public {
        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidICS23Proof.selector));
        ics23Harness.decodeNonMembershipProof(abi.encode(_membershipICS23Proof()), _nonMembershipProofPath());
    }

    function test_decodeNativeICS23NonMembershipRejectsWrongLeafKey() public {
        ICometBFTMsgs.ICS23Proof memory proof = _nonMembershipICS23Proof();
        proof.proofs[0].nonExistence.key = "clients/07-tendermint-0/wrong";

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidICS23Proof.selector));
        ics23Harness.decodeNonMembershipProof(abi.encode(proof), _nonMembershipProofPath());
    }

    function test_decodeNativeICS23ProofRejectsPopulatedInactiveOneofArm() public {
        ICometBFTMsgs.ICS23Proof memory proof = _membershipICS23Proof();
        proof.proofs[0].nonExistence.key = "inactive";

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidICS23Proof.selector));
        ics23Harness.decodeMembershipProof(abi.encode(proof), _proofPath(), "value");

        proof = _membershipICS23Proof();
        proof.proofs[0].nonExistence.left.proof.leaf.prefix = hex"ff";

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidICS23Proof.selector));
        ics23Harness.decodeMembershipProof(abi.encode(proof), _proofPath(), "value");

        proof = _nonMembershipICS23Proof();
        proof.proofs[0].existence = _existenceProof("clients/07-tendermint-0/clientState", "value");

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidICS23Proof.selector));
        ics23Harness.decodeNonMembershipProof(abi.encode(proof), _nonMembershipProofPath());

        proof = _nonMembershipICS23Proof();
        proof.proofs[0].existence.leaf.prefix = hex"ff";

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidICS23Proof.selector));
        ics23Harness.decodeNonMembershipProof(abi.encode(proof), _nonMembershipProofPath());

        proof = _nonMembershipICS23Proof();
        proof.proofs[0].nonExistence.left.proof.leaf.prefix = hex"ff";

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidICS23Proof.selector));
        ics23Harness.decodeNonMembershipProof(abi.encode(proof), _nonMembershipProofPath());
    }

    function test_decodeNativeICS23ProofRejectsUnsupportedProofType() public {
        ICometBFTMsgs.ICS23Proof memory proof = _membershipICS23Proof();
        proof.proofs[0].proofType = 3;

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.UnsupportedICS23ProofType.selector, 3));
        ics23Harness.decodeMembershipProof(abi.encode(proof), _proofPath(), "value");
    }

    function test_doubleSignMisbehaviourFreezesClient() public {
        client.misbehaviour(abi.encode(_doubleSignMisbehaviourMsg()));

        ICometBFTMsgs.ClientState memory frozenState = abi.decode(client.getClientState(), (ICometBFTMsgs.ClientState));
        assertTrue(frozenState.isFrozen);
    }

    function test_timeMonotonicityMisbehaviourFreezesClient() public {
        client.updateClient(abi.encode(_updateMsg()));

        client.misbehaviour(abi.encode(_timeViolationMisbehaviourMsg()));

        ICometBFTMsgs.ClientState memory frozenState = abi.decode(client.getClientState(), (ICometBFTMsgs.ClientState));
        assertTrue(frozenState.isFrozen);
    }

    function test_invalidMisbehaviourDoesNotFreezeClient() public {
        ICometBFTMsgs.MsgSubmitMisbehaviour memory msg_ =
            ICometBFTMsgs.MsgSubmitMisbehaviour({ updateA: _updateMsg(), updateB: _updateMsg() });

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidMisbehaviour.selector));
        client.misbehaviour(abi.encode(msg_));

        ICometBFTMsgs.ClientState memory state = abi.decode(client.getClientState(), (ICometBFTMsgs.ClientState));
        assertFalse(state.isFrozen);
        assertEq(uint256(client.updateClient(abi.encode(_updateMsg()))), uint256(ILightClientMsgs.UpdateResult.Update));
    }

    function test_invalidMisbehaviourSignatureDoesNotFreezeClient() public {
        ICometBFTMsgs.MsgSubmitMisbehaviour memory msg_ = _doubleSignMisbehaviourMsg();
        msg_.updateB.commit.signatures[0].signature[64] = bytes1(uint8(2));

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidSignatureV.selector, 2));
        client.misbehaviour(abi.encode(msg_));

        ICometBFTMsgs.ClientState memory state = abi.decode(client.getClientState(), (ICometBFTMsgs.ClientState));
        assertFalse(state.isFrozen);
    }

    function test_invalidMisbehaviourTrustedConsensusStateDoesNotFreezeClient() public {
        ICometBFTMsgs.MsgSubmitMisbehaviour memory msg_ = _doubleSignMisbehaviourMsg();
        msg_.updateB.trustedConsensusState.root = bytes32(uint256(1));
        bytes32 expected = client.getConsensusStateHash(msg_.updateB.trustedHeight);
        bytes32 actual = keccak256(abi.encode(msg_.updateB.trustedConsensusState));

        vm.expectRevert(
            abi.encodeWithSelector(ICometBFTClientErrors.ConsensusStateHashMismatch.selector, expected, actual)
        );
        client.misbehaviour(abi.encode(msg_));

        ICometBFTMsgs.ClientState memory state = abi.decode(client.getClientState(), (ICometBFTMsgs.ClientState));
        assertFalse(state.isFrozen);
    }

    function test_invalidMisbehaviourTrustedRevisionDoesNotFreezeClient() public {
        ICometBFTMsgs.MsgSubmitMisbehaviour memory msg_ = _doubleSignMisbehaviourMsg();
        msg_.updateB.trustedHeight.revisionNumber += 1;

        vm.expectRevert(
            abi.encodeWithSelector(
                ICometBFTClientErrors.RevisionNumberMismatch.selector,
                _clientState().latestHeight.revisionNumber,
                msg_.updateB.trustedHeight.revisionNumber
            )
        );
        client.misbehaviour(abi.encode(msg_));

        ICometBFTMsgs.ClientState memory state = abi.decode(client.getClientState(), (ICometBFTMsgs.ClientState));
        assertFalse(state.isFrozen);
    }

    function test_invalidMisbehaviourChainIDDoesNotFreezeClient() public {
        ICometBFTMsgs.MsgSubmitMisbehaviour memory msg_ = _doubleSignMisbehaviourMsg();
        msg_.updateB.header.chainId = "wrong-chain";

        vm.expectRevert(
            abi.encodeWithSelector(
                ICometBFTClientErrors.ChainIdMismatch.selector, _clientState().chainId, msg_.updateB.header.chainId
            )
        );
        client.misbehaviour(abi.encode(msg_));

        ICometBFTMsgs.ClientState memory state = abi.decode(client.getClientState(), (ICometBFTMsgs.ClientState));
        assertFalse(state.isFrozen);
    }

    function test_malformedNonAdjacentMisbehaviourEvidenceDoesNotFreezeClient() public {
        ICometBFTMsgs.MsgSubmitMisbehaviour memory msg_ = _doubleSignMisbehaviourMsg();
        msg_.updateB.header.height += 1;
        bytes32 headerHash = CometBFTProto.headerHash(msg_.updateB.header);

        vm.expectRevert(
            abi.encodeWithSelector(
                ICometBFTClientErrors.HeaderCommitHashMismatch.selector, headerHash, msg_.updateB.commit.blockId.hash
            )
        );
        client.misbehaviour(abi.encode(msg_));

        ICometBFTMsgs.ClientState memory state = abi.decode(client.getClientState(), (ICometBFTMsgs.ClientState));
        assertFalse(state.isFrozen);
    }

    function test_insufficientMisbehaviourQuorumDoesNotFreezeClient() public {
        ICometBFTMsgs.MsgSubmitMisbehaviour memory msg_ = _doubleSignMisbehaviourMsg();
        msg_.updateB.commit.signatures[0] = ICometBFTMsgs.CommitSig({
            blockIdFlag: 1, validatorAddress: address(0), timestampSeconds: 0, timestampNanos: 0, signature: ""
        });
        uint256 totalVotingPower;
        uint256 signedVotingPower;
        for (uint256 i = 0; i < msg_.updateB.validators.length; ++i) {
            totalVotingPower += msg_.updateB.validators[i].votingPower;
            if (i != 0) {
                signedVotingPower += msg_.updateB.validators[i].votingPower;
            }
        }
        uint256 votingPowerNeeded = totalVotingPower * 2 / 3;

        vm.expectRevert(
            abi.encodeWithSelector(
                ICometBFTClientErrors.NotEnoughVotingPower.selector, signedVotingPower, votingPowerNeeded
            )
        );
        client.misbehaviour(abi.encode(msg_));

        ICometBFTMsgs.ClientState memory state = abi.decode(client.getClientState(), (ICometBFTMsgs.ClientState));
        assertFalse(state.isFrozen);
    }

    function test_frozenClientRejectsUpdateProofsAndMisbehaviour() public {
        client.misbehaviour(abi.encode(_doubleSignMisbehaviourMsg()));

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.FrozenClientState.selector));
        client.updateClient(abi.encode(_updateMsg()));

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.FrozenClientState.selector));
        client.verifyMembership(_membershipVerifyMsg());

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.FrozenClientState.selector));
        client.verifyNonMembership(_realNonMembershipVerifyMsg());

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.FrozenClientState.selector));
        client.misbehaviour(abi.encode(_doubleSignMisbehaviourMsg()));
    }

    function test_benchmarkCompressedWitnessPath() public view {
        _assertBenchmarkCompressedWitnessPath();
    }

    function test_benchmarkUncompressedPubKeyPath() public view {
        _assertBenchmarkUncompressedPubKeyPath();
    }

    function test_benchmarkTwentyValidatorCompressedWitnessPath() public {
        _loadFixture("native_update_20_validators_fixture.json");
        _assertBenchmarkCompressedWitnessPath();
    }

    function test_benchmarkTwentyValidatorUncompressedPubKeyPath() public {
        _loadFixture("native_update_20_validators_fixture.json");
        _assertBenchmarkUncompressedPubKeyPath();
    }

    function test_invalidValidatorPubKeyFails() public {
        ICometBFTMsgs.MsgUpdateClient memory msg_ = _updateMsg();
        msg_.validators[0].pubKey = hex"02";

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidValidatorPubKey.selector, 0));
        client.updateClient(abi.encode(msg_));
    }

    function test_invalidValidatorPubKeyWitnessFails() public {
        ICometBFTMsgs.MsgUpdateClient memory msg_ = _updateMsg();
        msg_.validators[0].y = bytes32(0);

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidValidatorPubKeyWitness.selector, 0));
        client.updateClient(abi.encode(msg_));
    }

    function test_invalidValidatorPubKeyWitnessParityFails() public {
        ICometBFTMsgs.MsgUpdateClient memory msg_ = _updateMsg();
        msg_.validators[0].pubKey[0] = msg_.validators[0].pubKey[0] == 0x02 ? bytes1(0x03) : bytes1(0x02);

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidValidatorPubKeyWitness.selector, 0));
        client.updateClient(abi.encode(msg_));
    }

    function _updateMsg() private view returns (ICometBFTMsgs.MsgUpdateClient memory) {
        return ICometBFTMsgs.MsgUpdateClient({
            trustedHeight: _trustedHeight(),
            trustedConsensusState: _trustedConsensusState(),
            header: _header(),
            commit: _commit(),
            validators: _validators(),
            nextValidators: _nextValidators()
        });
    }

    function _doubleSignMisbehaviourMsg() private view returns (ICometBFTMsgs.MsgSubmitMisbehaviour memory) {
        return
            ICometBFTMsgs.MsgSubmitMisbehaviour({ updateA: _updateMsg(), updateB: _misbehaviourUpdate(".doubleSign") });
    }

    function _timeViolationMisbehaviourMsg() private view returns (ICometBFTMsgs.MsgSubmitMisbehaviour memory) {
        return
            ICometBFTMsgs.MsgSubmitMisbehaviour({
                updateA: _updateMsg(), updateB: _misbehaviourUpdate(".timeViolation")
            });
    }

    function _misbehaviourUpdate(string memory prefix) private view returns (ICometBFTMsgs.MsgUpdateClient memory) {
        return ICometBFTMsgs.MsgUpdateClient({
            trustedHeight: IICS02ClientMsgs.Height({
                revisionNumber: uint64(misbehaviourFixtureJson.readUint(".revisionNumber")),
                revisionHeight: uint64(misbehaviourFixtureJson.readUint(string.concat(prefix, ".trustedHeight")))
            }),
            trustedConsensusState: ICometBFTMsgs.ConsensusState({
                timestamp: uint128(
                    misbehaviourFixtureJson.readUint(string.concat(prefix, ".trustedConsensusState.timestamp"))
                ),
                root: misbehaviourFixtureJson.readBytes32(string.concat(prefix, ".trustedConsensusState.root")),
                nextValidatorsHash: misbehaviourFixtureJson.readBytes32(
                    string.concat(prefix, ".trustedConsensusState.nextValidatorsHash")
                )
            }),
            header: _misbehaviourHeader(prefix),
            commit: _misbehaviourCommit(prefix),
            validators: _misbehaviourValidators(),
            nextValidators: _misbehaviourValidators()
        });
    }

    function _clientState() private view returns (ICometBFTMsgs.ClientState memory) {
        return ICometBFTMsgs.ClientState({
            chainId: fixtureJson.readString(".chainId"),
            trustLevel: ICometBFTMsgs.TrustThreshold({ numerator: 1, denominator: 3 }),
            latestHeight: _trustedHeight(),
            trustingPeriod: uint32(fixtureJson.readUint(".trustingPeriod")),
            unbondingPeriod: uint32(fixtureJson.readUint(".unbondingPeriod")),
            maxClockDrift: uint32(fixtureJson.readUint(".maxClockDrift")),
            isFrozen: false
        });
    }

    function _trustedHeight() private view returns (IICS02ClientMsgs.Height memory) {
        return IICS02ClientMsgs.Height({
            revisionNumber: uint64(fixtureJson.readUint(".revisionNumber")),
            revisionHeight: uint64(fixtureJson.readUint(".trustedHeight"))
        });
    }

    function _trustedConsensusState() private view returns (ICometBFTMsgs.ConsensusState memory) {
        return ICometBFTMsgs.ConsensusState({
            timestamp: uint128(fixtureJson.readUint(".trustedConsensusState.timestamp")),
            root: fixtureJson.readBytes32(".trustedConsensusState.root"),
            nextValidatorsHash: fixtureJson.readBytes32(".trustedConsensusState.nextValidatorsHash")
        });
    }

    function _header() private view returns (ICometBFTMsgs.Header memory) {
        return ICometBFTMsgs.Header({
            versionBlock: uint64(fixtureJson.readUint(".header.versionBlock")),
            versionApp: uint64(fixtureJson.readUint(".header.versionApp")),
            chainId: fixtureJson.readString(".header.chainId"),
            height: uint64(fixtureJson.readUint(".header.height")),
            timeSeconds: uint64(fixtureJson.readUint(".header.timeSeconds")),
            timeNanos: uint32(fixtureJson.readUint(".header.timeNanos")),
            lastBlockId: _blockID(".header.lastBlockId"),
            lastCommitHash: fixtureJson.readBytes32(".header.lastCommitHash"),
            dataHash: fixtureJson.readBytes32(".header.dataHash"),
            validatorsHash: fixtureJson.readBytes32(".header.validatorsHash"),
            nextValidatorsHash: fixtureJson.readBytes32(".header.nextValidatorsHash"),
            consensusHash: fixtureJson.readBytes32(".header.consensusHash"),
            appHash: fixtureJson.readBytes32(".header.appHash"),
            lastResultsHash: fixtureJson.readBytes32(".header.lastResultsHash"),
            evidenceHash: fixtureJson.readBytes32(".header.evidenceHash"),
            proposerAddress: fixtureJson.readAddress(".header.proposerAddress")
        });
    }

    function _commit() private view returns (ICometBFTMsgs.Commit memory) {
        uint256[] memory blockIDFlags = fixtureJson.readUintArray(".commit.blockIdFlags");
        address[] memory validatorAddresses = fixtureJson.readAddressArray(".commit.validatorAddresses");
        uint256[] memory timestampSeconds = fixtureJson.readUintArray(".commit.timestampSeconds");
        uint256[] memory timestampNanos = fixtureJson.readUintArray(".commit.timestampNanos");
        bytes[] memory signatures = fixtureJson.readBytesArray(".commit.signatures");

        require(validatorAddresses.length == blockIDFlags.length, "bad fixture validator addresses");
        require(timestampSeconds.length == blockIDFlags.length, "bad fixture timestamp seconds");
        require(timestampNanos.length == blockIDFlags.length, "bad fixture timestamp nanos");
        require(signatures.length == blockIDFlags.length, "bad fixture signatures");

        ICometBFTMsgs.CommitSig[] memory commitSigs = new ICometBFTMsgs.CommitSig[](blockIDFlags.length);
        for (uint256 i = 0; i < blockIDFlags.length; ++i) {
            commitSigs[i] = ICometBFTMsgs.CommitSig({
                blockIdFlag: uint8(blockIDFlags[i]),
                validatorAddress: validatorAddresses[i],
                timestampSeconds: uint64(timestampSeconds[i]),
                timestampNanos: uint32(timestampNanos[i]),
                signature: signatures[i]
            });
        }

        return ICometBFTMsgs.Commit({
            height: uint64(fixtureJson.readUint(".commit.height")),
            round: uint32(fixtureJson.readUint(".commit.round")),
            blockId: _blockID(".commit.blockId"),
            signatures: commitSigs
        });
    }

    function _misbehaviourHeader(string memory prefix) private view returns (ICometBFTMsgs.Header memory) {
        string memory key = string.concat(prefix, ".header");
        return ICometBFTMsgs.Header({
            versionBlock: uint64(misbehaviourFixtureJson.readUint(string.concat(key, ".versionBlock"))),
            versionApp: uint64(misbehaviourFixtureJson.readUint(string.concat(key, ".versionApp"))),
            chainId: misbehaviourFixtureJson.readString(string.concat(key, ".chainId")),
            height: uint64(misbehaviourFixtureJson.readUint(string.concat(key, ".height"))),
            timeSeconds: uint64(misbehaviourFixtureJson.readUint(string.concat(key, ".timeSeconds"))),
            timeNanos: uint32(misbehaviourFixtureJson.readUint(string.concat(key, ".timeNanos"))),
            lastBlockId: _misbehaviourBlockID(string.concat(key, ".lastBlockId")),
            lastCommitHash: misbehaviourFixtureJson.readBytes32(string.concat(key, ".lastCommitHash")),
            dataHash: misbehaviourFixtureJson.readBytes32(string.concat(key, ".dataHash")),
            validatorsHash: misbehaviourFixtureJson.readBytes32(string.concat(key, ".validatorsHash")),
            nextValidatorsHash: misbehaviourFixtureJson.readBytes32(string.concat(key, ".nextValidatorsHash")),
            consensusHash: misbehaviourFixtureJson.readBytes32(string.concat(key, ".consensusHash")),
            appHash: misbehaviourFixtureJson.readBytes32(string.concat(key, ".appHash")),
            lastResultsHash: misbehaviourFixtureJson.readBytes32(string.concat(key, ".lastResultsHash")),
            evidenceHash: misbehaviourFixtureJson.readBytes32(string.concat(key, ".evidenceHash")),
            proposerAddress: misbehaviourFixtureJson.readAddress(string.concat(key, ".proposerAddress"))
        });
    }

    function _misbehaviourCommit(string memory prefix) private view returns (ICometBFTMsgs.Commit memory) {
        string memory key = string.concat(prefix, ".commit");
        uint256[] memory blockIDFlags = misbehaviourFixtureJson.readUintArray(string.concat(key, ".blockIdFlags"));
        address[] memory validatorAddresses =
            misbehaviourFixtureJson.readAddressArray(string.concat(key, ".validatorAddresses"));
        uint256[] memory timestampSeconds =
            misbehaviourFixtureJson.readUintArray(string.concat(key, ".timestampSeconds"));
        uint256[] memory timestampNanos = misbehaviourFixtureJson.readUintArray(string.concat(key, ".timestampNanos"));
        bytes[] memory signatures = misbehaviourFixtureJson.readBytesArray(string.concat(key, ".signatures"));

        require(validatorAddresses.length == blockIDFlags.length, "bad misbehaviour validator addresses");
        require(timestampSeconds.length == blockIDFlags.length, "bad misbehaviour timestamp seconds");
        require(timestampNanos.length == blockIDFlags.length, "bad misbehaviour timestamp nanos");
        require(signatures.length == blockIDFlags.length, "bad misbehaviour signatures");

        ICometBFTMsgs.CommitSig[] memory commitSigs = new ICometBFTMsgs.CommitSig[](blockIDFlags.length);
        for (uint256 i = 0; i < blockIDFlags.length; ++i) {
            commitSigs[i] = ICometBFTMsgs.CommitSig({
                blockIdFlag: uint8(blockIDFlags[i]),
                validatorAddress: validatorAddresses[i],
                timestampSeconds: uint64(timestampSeconds[i]),
                timestampNanos: uint32(timestampNanos[i]),
                signature: signatures[i]
            });
        }

        return ICometBFTMsgs.Commit({
            height: uint64(misbehaviourFixtureJson.readUint(string.concat(key, ".height"))),
            round: uint32(misbehaviourFixtureJson.readUint(string.concat(key, ".round"))),
            blockId: _misbehaviourBlockID(string.concat(key, ".blockId")),
            signatures: commitSigs
        });
    }

    function _validators() private view returns (ICometBFTMsgs.Validator[] memory) {
        return _validators(".validators");
    }

    function _trustedValidators() private view returns (ICometBFTMsgs.Validator[] memory) {
        return _validators(".trustedValidators");
    }

    function _nextValidators() private view returns (ICometBFTMsgs.Validator[] memory) {
        return _validators(".nextValidators");
    }

    function _emptyValidators() private pure returns (ICometBFTMsgs.Validator[] memory) {
        return new ICometBFTMsgs.Validator[](0);
    }

    function _validators(string memory key) private view returns (ICometBFTMsgs.Validator[] memory) {
        bytes[] memory publicKeys = fixtureJson.readBytesArray(string.concat(key, ".publicKeys"));
        bytes[] memory yWitnesses = fixtureJson.readBytesArray(string.concat(key, ".publicKeyYWitnesses"));
        uint256[] memory votingPowers = fixtureJson.readUintArray(string.concat(key, ".votingPowers"));
        require(publicKeys.length == votingPowers.length, "bad fixture validators");
        require(yWitnesses.length == votingPowers.length, "bad fixture validator witnesses");

        ICometBFTMsgs.Validator[] memory validators = new ICometBFTMsgs.Validator[](publicKeys.length);
        for (uint256 i = 0; i < publicKeys.length; ++i) {
            validators[i] = ICometBFTMsgs.Validator({
                pubKey: publicKeys[i], y: _readBytes32(yWitnesses[i]), votingPower: uint64(votingPowers[i])
            });
        }
        return validators;
    }

    function _misbehaviourValidators() private view returns (ICometBFTMsgs.Validator[] memory) {
        bytes[] memory publicKeys = misbehaviourFixtureJson.readBytesArray(".validators.publicKeys");
        bytes[] memory yWitnesses = misbehaviourFixtureJson.readBytesArray(".validators.publicKeyYWitnesses");
        uint256[] memory votingPowers = misbehaviourFixtureJson.readUintArray(".validators.votingPowers");
        require(publicKeys.length == votingPowers.length, "bad misbehaviour validators");
        require(yWitnesses.length == votingPowers.length, "bad misbehaviour validator witnesses");

        ICometBFTMsgs.Validator[] memory validators = new ICometBFTMsgs.Validator[](publicKeys.length);
        for (uint256 i = 0; i < publicKeys.length; ++i) {
            validators[i] = ICometBFTMsgs.Validator({
                pubKey: publicKeys[i], y: _readBytes32(yWitnesses[i]), votingPower: uint64(votingPowers[i])
            });
        }
        return validators;
    }

    function _pubKeyValidators() private view returns (CometBFTGasBenchmark.EthPubKeyValidator[] memory) {
        bytes[] memory publicKeys = fixtureJson.readBytesArray(".validators.uncompressedPublicKeys");
        uint256[] memory votingPowers = fixtureJson.readUintArray(".validators.votingPowers");
        require(publicKeys.length == votingPowers.length, "bad fixture validator pubkeys");

        CometBFTGasBenchmark.EthPubKeyValidator[] memory validators =
            new CometBFTGasBenchmark.EthPubKeyValidator[](publicKeys.length);
        for (uint256 i = 0; i < publicKeys.length; ++i) {
            validators[i] = CometBFTGasBenchmark.EthPubKeyValidator({
                pubKey: publicKeys[i], votingPower: uint64(votingPowers[i])
            });
        }
        return validators;
    }

    function _blockID(string memory key) private view returns (ICometBFTMsgs.BlockID memory) {
        return ICometBFTMsgs.BlockID({
            hash: fixtureJson.readBytes32(string.concat(key, ".hash")),
            partSetHeader: ICometBFTMsgs.PartSetHeader({
                total: uint32(fixtureJson.readUint(string.concat(key, ".partSetHeader.total"))),
                hash: fixtureJson.readBytes32(string.concat(key, ".partSetHeader.hash"))
            })
        });
    }

    function _misbehaviourBlockID(string memory key) private view returns (ICometBFTMsgs.BlockID memory) {
        return ICometBFTMsgs.BlockID({
            hash: misbehaviourFixtureJson.readBytes32(string.concat(key, ".hash")),
            partSetHeader: ICometBFTMsgs.PartSetHeader({
                total: uint32(misbehaviourFixtureJson.readUint(string.concat(key, ".partSetHeader.total"))),
                hash: misbehaviourFixtureJson.readBytes32(string.concat(key, ".partSetHeader.hash"))
            })
        });
    }

    function _newConsensusState(ICometBFTMsgs.Header memory header_)
        private
        pure
        returns (ICometBFTMsgs.ConsensusState memory)
    {
        return ICometBFTMsgs.ConsensusState({
            timestamp: uint128(header_.timeSeconds) * 1e9 + uint128(header_.timeNanos),
            root: header_.appHash,
            nextValidatorsHash: header_.nextValidatorsHash
        });
    }

    function _makeAbsent(ICometBFTMsgs.CommitSig memory sig) private pure {
        sig.blockIdFlag = 1;
        sig.validatorAddress = address(0);
        sig.timestampSeconds = 0;
        sig.timestampNanos = 0;
        sig.signature = "";
    }

    function _readBytes32(bytes memory bz) private pure returns (bytes32 value) {
        require(bz.length == 32, "bad bytes32 fixture value");
        assembly ("memory-safe") {
            value := mload(add(bz, 32))
        }
    }

    function _assertFixtureVectorsMatch() private view {
        ICometBFTMsgs.Validator[] memory validators = _validators();
        ICometBFTMsgs.Validator[] memory trustedValidators = _trustedValidators();
        ICometBFTMsgs.Header memory header_ = _header();
        ICometBFTMsgs.Commit memory commit_ = _commit();

        bytes32 trustedValidatorSetHash = CometBFTProto.validatorSetHash(trustedValidators);
        assertEq(trustedValidatorSetHash, fixtureJson.readBytes32(".expected.trustedValidatorSetHash"));
        assertEq(trustedValidatorSetHash, _trustedConsensusState().nextValidatorsHash);

        bytes32 validatorSetHash = CometBFTProto.validatorSetHash(validators);
        assertEq(validatorSetHash, fixtureJson.readBytes32(".expected.validatorSetHash"));
        assertEq(validatorSetHash, header_.validatorsHash);

        bytes32 nextValidatorSetHash = CometBFTProto.validatorSetHash(_nextValidators());
        assertEq(nextValidatorSetHash, fixtureJson.readBytes32(".expected.nextValidatorSetHash"));
        assertEq(nextValidatorSetHash, header_.nextValidatorsHash);

        bytes32 headerHash = CometBFTProto.headerHash(header_);
        assertEq(headerHash, fixtureJson.readBytes32(".expected.headerHash"));
        assertEq(headerHash, commit_.blockId.hash);

        bytes[] memory expectedVoteSignBytes = fixtureJson.readBytesArray(".expected.voteSignBytes");
        address[] memory expectedSigners = fixtureJson.readAddressArray(".expected.recoveredSigners");
        for (uint256 i = 0; i < commit_.signatures.length; ++i) {
            bytes memory signBytes = CometBFTProto.voteSignBytes(header_.chainId, commit_, commit_.signatures[i]);
            assertEq(signBytes, expectedVoteSignBytes[i]);
            assertEq(CometBFTECDSA.recover(keccak256(signBytes), commit_.signatures[i].signature), expectedSigners[i]);
        }
    }

    function _assertValidSkippingFixtureShape() private view {
        assertGt(_header().height, _trustedHeight().revisionHeight + 1);
        assertNotEq(_header().validatorsHash, _trustedConsensusState().nextValidatorsHash);
        assertGt(
            fixtureJson.readUint(".expected.trustedSignedVotingPower"),
            fixtureJson.readUint(".expected.trustedVotingPowerNeeded")
        );
        assertGt(
            fixtureJson.readUint(".expected.newSignedVotingPower"),
            fixtureJson.readUint(".expected.newVotingPowerNeeded")
        );
        _assertFixtureVectorsMatch();
    }

    function _assertValidAdjacentUpdateClient() private {
        ICometBFTMsgs.MsgUpdateClient memory msg_ = _updateMsg();

        ILightClientMsgs.UpdateResult result = client.updateClient(abi.encode(msg_));
        assertEq(uint256(result), uint256(ILightClientMsgs.UpdateResult.Update));

        ICometBFTMsgs.ClientState memory nextClientState =
            abi.decode(client.getClientState(), (ICometBFTMsgs.ClientState));
        assertEq(nextClientState.chainId, fixtureJson.readString(".chainId"));
        assertEq(nextClientState.latestHeight.revisionHeight, msg_.header.height);
        assertFalse(nextClientState.isFrozen);

        ICometBFTMsgs.ConsensusState memory expectedConsensusState = _newConsensusState(msg_.header);
        IICS02ClientMsgs.Height memory expectedHeight = IICS02ClientMsgs.Height({
            revisionNumber: _trustedHeight().revisionNumber, revisionHeight: msg_.header.height
        });
        assertEq(client.getConsensusStateHash(expectedHeight), keccak256(abi.encode(expectedConsensusState)));
        assertEq(
            client.getConsensusStateHash(expectedHeight.revisionHeight), keccak256(abi.encode(expectedConsensusState))
        );
        _assertConsensusStateEq(client.getConsensusState(expectedHeight), expectedConsensusState);
    }

    function _membershipVerifyMsg() private view returns (ILightClientMsgs.MsgVerifyMembership memory) {
        return ILightClientMsgs.MsgVerifyMembership({
            proof: ics23FixtureJson.readBytes(".membership.proof"),
            proofHeight: _trustedHeight(),
            path: ics23FixtureJson.readBytesArray(".membership.path"),
            value: ics23FixtureJson.readBytes(".membership.value")
        });
    }

    function _membershipVerifyMsg(ICometBFTMsgs.ICS23Proof memory proof)
        private
        view
        returns (ILightClientMsgs.MsgVerifyMembership memory)
    {
        return ILightClientMsgs.MsgVerifyMembership({
            proof: abi.encode(proof),
            proofHeight: _trustedHeight(),
            path: ics23FixtureJson.readBytesArray(".membership.path"),
            value: ics23FixtureJson.readBytes(".membership.value")
        });
    }

    function _membershipProofFromFixture() private view returns (ICometBFTMsgs.ICS23Proof memory) {
        return abi.decode(ics23FixtureJson.readBytes(".membership.proof"), (ICometBFTMsgs.ICS23Proof));
    }

    function _membershipFixtureClient(bool frozen, bytes32 root) private returns (CometBFTClient) {
        ICometBFTMsgs.ClientState memory initialClientState = _clientState();
        initialClientState.isFrozen = frozen;

        ICometBFTMsgs.ConsensusState memory initialConsensusState = _trustedConsensusState();
        initialConsensusState.root = root;

        return new CometBFTClient(initialClientState, initialConsensusState, _trustedValidators(), address(this));
    }

    function _realMembershipVerifyMsg() private view returns (ILightClientMsgs.MsgVerifyMembership memory) {
        return ILightClientMsgs.MsgVerifyMembership({
            proof: realMembershipFixtureJson.readBytes(".membership.proof"),
            proofHeight: _realMembershipHeight(),
            path: realMembershipFixtureJson.readBytesArray(".membership.path"),
            value: realMembershipFixtureJson.readBytes(".membership.value")
        });
    }

    function _realMembershipFixtureClient(bool frozen) private returns (CometBFTClient) {
        ICometBFTMsgs.ClientState memory initialClientState = _clientState();
        initialClientState.latestHeight = _realMembershipHeight();
        initialClientState.isFrozen = frozen;

        ICometBFTMsgs.ConsensusState memory initialConsensusState = ICometBFTMsgs.ConsensusState({
            timestamp: uint128(realMembershipFixtureJson.readUint(".membership.timestamp")),
            root: realMembershipFixtureJson.readBytes32(".membership.root"),
            nextValidatorsHash: realMembershipFixtureJson.readBytes32(".membership.nextValidatorsHash")
        });

        return new CometBFTClient(initialClientState, initialConsensusState, _emptyValidators(), address(this));
    }

    function _realMembershipHeight() private view returns (IICS02ClientMsgs.Height memory) {
        return IICS02ClientMsgs.Height({
            revisionNumber: _trustedHeight().revisionNumber,
            revisionHeight: uint64(realMembershipFixtureJson.readUint(".membership.proofHeight"))
        });
    }

    function _realNonMembershipVerifyMsg() private view returns (ILightClientMsgs.MsgVerifyNonMembership memory) {
        return ILightClientMsgs.MsgVerifyNonMembership({
            proof: realNonMembershipFixtureJson.readBytes(".nonMembership.proof"),
            proofHeight: _realNonMembershipHeight(),
            path: realNonMembershipFixtureJson.readBytesArray(".nonMembership.path")
        });
    }

    function _realNonMembershipProofFromFixture() private view returns (ICometBFTMsgs.ICS23Proof memory) {
        return abi.decode(realNonMembershipFixtureJson.readBytes(".nonMembership.proof"), (ICometBFTMsgs.ICS23Proof));
    }

    function _realNonMembershipFixtureClient(bool frozen) private returns (CometBFTClient) {
        return _realNonMembershipFixtureClient(frozen, realNonMembershipFixtureJson.readBytes32(".nonMembership.root"));
    }

    function _realNonMembershipFixtureClient(bool frozen, bytes32 root) private returns (CometBFTClient) {
        ICometBFTMsgs.ClientState memory initialClientState = _clientState();
        initialClientState.latestHeight = _realNonMembershipHeight();
        initialClientState.isFrozen = frozen;

        ICometBFTMsgs.ConsensusState memory initialConsensusState = ICometBFTMsgs.ConsensusState({
            timestamp: uint128(realNonMembershipFixtureJson.readUint(".nonMembership.timestamp")),
            root: root,
            nextValidatorsHash: realNonMembershipFixtureJson.readBytes32(".nonMembership.nextValidatorsHash")
        });

        return new CometBFTClient(initialClientState, initialConsensusState, _emptyValidators(), address(this));
    }

    function _realNonMembershipHeight() private view returns (IICS02ClientMsgs.Height memory) {
        return IICS02ClientMsgs.Height({
            revisionNumber: _trustedHeight().revisionNumber,
            revisionHeight: uint64(realNonMembershipFixtureJson.readUint(".nonMembership.proofHeight"))
        });
    }

    function _proofPath() private pure returns (bytes[] memory path) {
        path = new bytes[](2);
        path[0] = "ibc";
        path[1] = "clients/07-tendermint-0/clientState";
    }

    function _nonMembershipProofPath() private pure returns (bytes[] memory path) {
        path = new bytes[](2);
        path[0] = "ibc";
        path[1] = "clients/07-tendermint-0/missing";
    }

    function _membershipICS23Proof() private pure returns (ICometBFTMsgs.ICS23Proof memory proof) {
        proof.proofs = new ICometBFTMsgs.ICS23CommitmentProof[](2);
        proof.proofs[0].proofType = 1;
        proof.proofs[0].existence = _existenceProof("clients/07-tendermint-0/clientState", "value");
        proof.proofs[1].proofType = 1;
        proof.proofs[1].existence = _existenceProof("ibc", "store-root");
    }

    function _nonMembershipICS23Proof() private pure returns (ICometBFTMsgs.ICS23Proof memory proof) {
        proof.proofs = new ICometBFTMsgs.ICS23CommitmentProof[](2);
        proof.proofs[0].proofType = 2;
        proof.proofs[0].nonExistence.key = "clients/07-tendermint-0/missing";
        proof.proofs[0].nonExistence.right.exists = true;
        proof.proofs[0].nonExistence.right.proof = _existenceProof("clients/07-tendermint-0/next", "value");
        proof.proofs[1].proofType = 1;
        proof.proofs[1].existence = _existenceProof("ibc", "store-root");
    }

    function _existenceProof(
        bytes memory key,
        bytes memory value
    )
        private
        pure
        returns (ICometBFTMsgs.ICS23ExistenceProof memory proof)
    {
        proof.key = key;
        proof.value = value;
        proof.hasLeaf = true;
        proof.leaf =
            ICometBFTMsgs.ICS23LeafOp({ hash: 1, prehashKey: 0, prehashValue: 1, length: 1, prefix: hex"000000" });
        proof.path = new ICometBFTMsgs.ICS23InnerOp[](1);
        proof.path[0] = ICometBFTMsgs.ICS23InnerOp({ hash: 1, prefix: hex"02000020", suffix: hex"" });
    }

    function _assertConsensusStateEq(
        ICometBFTMsgs.ConsensusState memory actual,
        ICometBFTMsgs.ConsensusState memory expected
    )
        private
        pure
    {
        assertEq(actual.timestamp, expected.timestamp);
        assertEq(actual.root, expected.root);
        assertEq(actual.nextValidatorsHash, expected.nextValidatorsHash);
    }

    function _assertBenchmarkCompressedWitnessPath() private view {
        (bytes32 validatorSetHash, bytes32 identityHash) = gasBenchmark.compressedWitnessPath(_validators());

        assertEq(validatorSetHash, fixtureJson.readBytes32(".expected.validatorSetHash"));
        assertNotEq(identityHash, bytes32(0));
    }

    function _assertBenchmarkUncompressedPubKeyPath() private view {
        (bytes32 compressedValidatorSetHash, bytes32 compressedIdentityHash) =
            gasBenchmark.compressedWitnessPath(_validators());
        (bytes32 pubKeyValidatorSetHash, bytes32 pubKeyIdentityHash) =
            gasBenchmark.uncompressedPubKeyPath(_pubKeyValidators());

        assertNotEq(pubKeyValidatorSetHash, compressedValidatorSetHash);
        assertEq(pubKeyIdentityHash, compressedIdentityHash);
    }

    function _fixturePath(string memory fileName) private view returns (string memory) {
        string memory projectRootPath = vm.projectRoot();
        string memory path = string.concat(projectRootPath, "/test/cometbft/fixtures/", fileName);
        if (vm.exists(path)) {
            return path;
        }
        return string.concat(projectRootPath, "/../test/cometbft/fixtures/", fileName);
    }
}
