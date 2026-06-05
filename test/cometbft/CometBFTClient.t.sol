// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { Test } from "forge-std/Test.sol";
import { stdJson } from "forge-std/StdJson.sol";

import { CometBFTClient } from "../../contracts/light-clients/cometbft/CometBFTClient.sol";
import { ICometBFTClientErrors } from "../../contracts/light-clients/cometbft/errors/ICometBFTClientErrors.sol";
import { ICometBFTMsgs } from "../../contracts/light-clients/cometbft/msgs/ICometBFTMsgs.sol";
import { CometBFTECDSA } from "../../contracts/light-clients/cometbft/utils/CometBFTECDSA.sol";
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

contract CometBFTClientTest is Test {
    using stdJson for string;

    string private fixtureJson;
    CometBFTClient private client;
    CometBFTGasBenchmark private gasBenchmark;

    function setUp() public {
        gasBenchmark = new CometBFTGasBenchmark();
        _loadFixture("native_update_fixture.json");
    }

    function _loadFixture(string memory fileName) private {
        fixtureJson = vm.readFile(_fixturePath(fileName));
        vm.warp(fixtureJson.readUint(".proofNow"));

        client = new CometBFTClient(_clientState(), _trustedConsensusState(), address(this));
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

    function test_validAdjacentUpdateClientReplayIsNoOp() public {
        ICometBFTMsgs.MsgUpdateClient memory msg_ = _updateMsg();

        ILightClientMsgs.UpdateResult result = client.updateClient(abi.encode(msg_));
        assertEq(uint256(result), uint256(ILightClientMsgs.UpdateResult.Update));

        result = client.updateClient(abi.encode(msg_));
        assertEq(uint256(result), uint256(ILightClientMsgs.UpdateResult.NoOp));
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

    function test_validatorSetMustUseCometBFTOrdering() public {
        ICometBFTMsgs.MsgUpdateClient memory msg_ = _updateMsg();
        ICometBFTMsgs.Validator memory first = msg_.validators[0];
        msg_.validators[0] = msg_.validators[1];
        msg_.validators[1] = first;

        vm.expectRevert(abi.encodeWithSelector(ICometBFTClientErrors.InvalidValidatorOrdering.selector, 1));
        client.updateClient(abi.encode(msg_));
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
            validators: _validators()
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

    function _validators() private view returns (ICometBFTMsgs.Validator[] memory) {
        bytes[] memory publicKeys = fixtureJson.readBytesArray(".validators.publicKeys");
        bytes[] memory yWitnesses = fixtureJson.readBytesArray(".validators.publicKeyYWitnesses");
        uint256[] memory votingPowers = fixtureJson.readUintArray(".validators.votingPowers");
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
        ICometBFTMsgs.Header memory header_ = _header();
        ICometBFTMsgs.Commit memory commit_ = _commit();

        bytes32 validatorSetHash = CometBFTProto.validatorSetHash(validators);
        assertEq(validatorSetHash, fixtureJson.readBytes32(".expected.validatorSetHash"));
        assertEq(validatorSetHash, header_.validatorsHash);

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
        assertEq(client.getConsensusStateHash(msg_.header.height), keccak256(abi.encode(expectedConsensusState)));
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
