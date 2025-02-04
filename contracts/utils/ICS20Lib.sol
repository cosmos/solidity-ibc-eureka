// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.28;

// solhint-disable no-inline-assembly

import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { Bytes } from "@openzeppelin-contracts/utils/Bytes.sol";
import { IICS20Errors } from "../errors/IICS20Errors.sol";
import { IICS26RouterMsgs } from "../msgs/IICS26RouterMsgs.sol";
import { IICS20TransferMsgs } from "../msgs/IICS20TransferMsgs.sol";
import { IBCERC20 } from "./IBCERC20.sol";

// This library was originally copied, with minor adjustments, from https://github.com/hyperledger-labs/yui-ibc-solidity
// It has since been modified heavily (e.g. replacing JSON with ABI encoding, adding new functions, etc.)
library ICS20Lib {
    /// @notice FungibleTokenPacketData is the payload for a fungible token transfer packet.
    /// @dev PacketData is defined in
    /// [ICS-20](https://github.com/cosmos/ibc/tree/main/spec/app/ics-020-fungible-token-transfer).
    /// @param denom The denomination of the token
    /// @param sender The sender of the token
    /// @param receiver The receiver of the token
    /// @param amount The amount of tokens
    /// @param memo Optional memo
    struct FungibleTokenPacketData {
        string denom;
        string sender;
        string receiver;
        uint256 amount;
        string memo;
    }

    /// @notice ICS20_VERSION is the version string for ICS20 packet data.
    string public constant ICS20_VERSION = "ics20-1";

    /// @notice ICS20_ENCODING is the encoding string for ICS20 packet data.
    string public constant ICS20_ENCODING = "application/x-solidity-abi";

    /// @notice IBC_DENOM_PREFIX is the prefix for IBC denoms.
    string public constant IBC_DENOM_PREFIX = "ibc/";

    /// @notice DEFAULT_PORT_ID is the default port id for ICS20.
    string public constant DEFAULT_PORT_ID = "transfer";

    /// @notice SUCCESSFUL_ACKNOWLEDGEMENT_JSON is the JSON bytes for a successful acknowledgement.
    bytes public constant SUCCESSFUL_ACKNOWLEDGEMENT_JSON = bytes("{\"result\":\"AQ==\"}");
    /// @notice FAILED_ACKNOWLEDGEMENT_JSON is the JSON bytes for a failed acknowledgement.
    bytes public constant FAILED_ACKNOWLEDGEMENT_JSON = bytes("{\"error\":\"failed\"}");
    /// @notice KECCAK256_SUCCESSFUL_ACKNOWLEDGEMENT_JSON is the keccak256 hash of SUCCESSFUL_ACKNOWLEDGEMENT_JSON.
    bytes32 internal constant KECCAK256_SUCCESSFUL_ACKNOWLEDGEMENT_JSON = keccak256(SUCCESSFUL_ACKNOWLEDGEMENT_JSON);

    /// @notice A dummy function to generate the ABI for the parameters.
    /// @param o1 The FungibleTokenPacketData.
    function abiPublicTypes(FungibleTokenPacketData memory o1) public pure 
    // solhint-disable-next-line no-empty-blocks
    {
        // This is a dummy function to generate the ABI for outputs
        // so that it can be used in the SP1 verifier contract.
        // The function is not used in the contract.
    }

    /// @notice Create an ICS26RouterMsgs.MsgSendPacket message for ics20-1.
    /// @notice This is a helper function for constructing the MsgSendPacket for ICS26Router.
    /// @param sender The sender of the transfer
    /// @param msg_ The message for sending a transfer
    /// @return The constructed MsgSendPacket
    function newMsgSendPacketV1(
        address sender,
        IICS20TransferMsgs.SendTransferMsg memory msg_
    )
        external
        view
        returns (IICS26RouterMsgs.MsgSendPacket memory)
    {
        require(msg_.amount > 0, IICS20Errors.ICS20InvalidAmount(msg_.amount));

        string memory fullDenomPath;
        try IBCERC20(mustHexStringToAddress(msg_.denom)).fullDenomPath() returns (string memory ibcERC20FullDenomPath) {
            // if the address is one of our IBCERC20 contracts, we get the correct denom for the packet there
            fullDenomPath = ibcERC20FullDenomPath;
        } catch {
            // otherwise this is just an ERC20 address, so we use it as the denom
            fullDenomPath = msg_.denom;
        }

        // We are encoding the payload in ABI format
        bytes memory packetData = abi.encode(
            ICS20Lib.FungibleTokenPacketData({
                denom: fullDenomPath,
                sender: Strings.toHexString(sender),
                receiver: msg_.receiver,
                amount: msg_.amount,
                memo: msg_.memo
            })
        );

        IICS26RouterMsgs.Payload[] memory payloads = new IICS26RouterMsgs.Payload[](1);
        payloads[0] = IICS26RouterMsgs.Payload({
            sourcePort: ICS20Lib.DEFAULT_PORT_ID,
            destPort: msg_.destPort,
            version: ICS20Lib.ICS20_VERSION,
            encoding: ICS20Lib.ICS20_ENCODING,
            value: packetData
        });
        return IICS26RouterMsgs.MsgSendPacket({
            sourceClient: msg_.sourceClient,
            timeoutTimestamp: msg_.timeoutTimestamp,
            payloads: payloads
        });
    }

    /// @notice hexStringToAddress converts a hex string to an address.
    /// @param addrHexString hex address string
    /// @return address value
    /// @return true if the conversion was successful
    function hexStringToAddress(string memory addrHexString) internal pure returns (address, bool) {
        (bool success, address addr) = Strings.tryParseAddress(addrHexString);
        return (addr, success);
    }

    /// @notice mustHexStringToAddress converts a hex string to an address and reverts on failure.
    /// @param addrHexString hex address string
    /// @return address the converted address
    function mustHexStringToAddress(string memory addrHexString) internal pure returns (address) {
        (bool success, address addr) = Strings.tryParseAddress(addrHexString);
        require(success, IICS20Errors.ICS20InvalidAddress(addrHexString));
        return addr;
    }

    /// @notice equal returns true if two byte arrays are equal.
    /// @param a bytes
    /// @param b bytes
    /// @return true if the byte arrays are equal
    function equal(bytes memory a, bytes memory b) internal pure returns (bool) {
        return keccak256(a) == keccak256(b);
    }

    /// @notice hasPrefix checks a denom for a prefix
    /// @param denomBz the denom to check
    /// @param prefix the prefix to check with
    /// @return true if `denomBz` has the prefix `prefix`
    function hasPrefix(bytes memory denomBz, bytes memory prefix) internal pure returns (bool) {
        if (denomBz.length < prefix.length) {
            return false;
        }
        return equal(Bytes.slice(denomBz, 0, prefix.length), prefix);
    }

    /// @notice errorAck returns an error acknowledgement.
    /// @param reason Error reason
    /// @return Error acknowledgement
    function errorAck(bytes memory reason) internal pure returns (bytes memory) {
        return abi.encodePacked("{\"error\":\"", reason, "\"}");
    }

    /// @notice getDenomPrefix returns an ibc path prefix
    /// @param portId Port
    /// @param clientId client
    /// @return Denom prefix
    function getDenomPrefix(string calldata portId, string calldata clientId) internal pure returns (bytes memory) {
        return abi.encodePacked(portId, "/", clientId, "/");
    }

    /// @notice removeHop removes the first hop from the denom trace.
    /// @param denom Denom to remove the hop from
    /// @param hop Hop to remove (it must be the first hop)
    /// @return The new denom with the first hop removed
    function removeHop(bytes memory denom, bytes memory hop) internal pure returns (bytes memory) {
        return Bytes.slice(denom, hop.length);
    }

    /// @notice addHop adds a hop to the denom trace as the first hop.
    /// @param denom Denom to add the hop to
    /// @param hop Hop to add
    /// @return The new denom with the hop added
    function addHop(
        bytes memory denom,
        bytes memory hop
    )
        internal
        pure
        returns (bytes memory)
    {
        return abi.encodePacked(hop, denom);
    }

    /// @notice hasHops checks if a denom has any hops in it (i.e it has a "/" in it).
    /// @param denom Denom to check
    /// @return true if the denom has any hops in it
    function hasHops(bytes memory denom) internal pure returns (bool) {
        // check if the denom has any '/' in it
        for (uint256 i = 0; i < denom.length; i++) {
            if (denom[i] == "/") {
                return true;
            }
        }

        return false;
    }
}
