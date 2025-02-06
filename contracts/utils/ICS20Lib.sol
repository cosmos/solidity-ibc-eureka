// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.28;

// solhint-disable no-inline-assembly

import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { Bytes } from "@openzeppelin-contracts/utils/Bytes.sol";
import { IICS20Errors } from "../errors/IICS20Errors.sol";
import { IICS20TransferMsgs } from "../msgs/IICS20TransferMsgs.sol";
import { IBCERC20 } from "./IBCERC20.sol";

// This library was originally copied, with minor adjustments, from https://github.com/hyperledger-labs/yui-ibc-solidity
// It has since been modified heavily (e.g. replacing JSON with ABI encoding, adding new functions, etc.)
library ICS20Lib {
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

    /// @notice Create an ICS20Lib.FungibleTokenPacketData message for ics20-1.
    /// @param sender The sender of the transfer
    /// @param msg_ The message for sending a transfer
    /// @return The constructed MsgSendPacket
    function newFungibleTokenPacketDataV1(
        address sender,
        IICS20TransferMsgs.SendTransferMsg calldata msg_
    )
        internal
        view
        returns (IICS20TransferMsgs.FungibleTokenPacketData memory)
    {
        require(msg_.amount > 0, IICS20Errors.ICS20InvalidAmount(msg_.amount));

        string memory fullDenomPath;
        try IBCERC20(msg_.denom).fullDenomPath() returns (string memory ibcERC20FullDenomPath) {
            // if the address is one of our IBCERC20 contracts, we get the correct denom for the packet there
            fullDenomPath = ibcERC20FullDenomPath;
        } catch {
            // otherwise this is just an ERC20 address, so we use it as the denom
            fullDenomPath = Strings.toHexString(msg_.denom);
        }

        // We are encoding the payload in ABI format
        return IICS20TransferMsgs.FungibleTokenPacketData({
            denom: fullDenomPath,
            sender: Strings.toHexString(sender),
            receiver: msg_.receiver,
            amount: msg_.amount,
            memo: msg_.memo
        });
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
    function getDenomPrefix(string memory portId, string calldata clientId) internal pure returns (bytes memory) {
        return abi.encodePacked(portId, "/", clientId, "/");
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
