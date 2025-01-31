// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.28;

// solhint-disable no-inline-assembly

import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { IICS20Errors } from "../errors/IICS20Errors.sol";
import { IICS26RouterMsgs } from "../msgs/IICS26RouterMsgs.sol";
import { IICS20TransferMsgs } from "../msgs/IICS20TransferMsgs.sol";
import { IBCERC20 } from "./IBCERC20.sol";

// This library was originally copied, with minor adjustments, from https://github.com/hyperledger-labs/yui-ibc-solidity
// It has since been modified heavily (e.g. ICS20-2, replacing JSON with ABI encoding, adding new functions, etc.)
library ICS20Lib {
    using Strings for string;

    /// @notice ICS20_VERSION is the version string for ICS20 packet data.
    string public constant ICS20_VERSION = "ics20-2";

    /// @notice ICS20_ENCODING is the encoding string for ICS20 packet data.
    string public constant ICS20_ENCODING = "application/x-solidity-abi";

    /// @notice DEFAULT_PORT_ID is the default port id for ICS20.
    string public constant DEFAULT_PORT_ID = "transfer";

    /// @notice SUCCESSFUL_ACKNOWLEDGEMENT_JSON is the JSON bytes for a successful acknowledgement.
    bytes public constant SUCCESSFUL_ACKNOWLEDGEMENT_JSON = bytes("{\"result\":\"AQ==\"}");
    /// @notice FAILED_ACKNOWLEDGEMENT_JSON is the JSON bytes for a failed acknowledgement.
    bytes public constant FAILED_ACKNOWLEDGEMENT_JSON = bytes("{\"error\":\"failed\"}");
    /// @notice KECCAK256_SUCCESSFUL_ACKNOWLEDGEMENT_JSON is the keccak256 hash of SUCCESSFUL_ACKNOWLEDGEMENT_JSON.
    bytes32 internal constant KECCAK256_SUCCESSFUL_ACKNOWLEDGEMENT_JSON = keccak256(SUCCESSFUL_ACKNOWLEDGEMENT_JSON);

    /// @notice A dummy function to generate the ABI for the parameters.
    /// @param o1 The IICS20TransferMsgs.FungibleTokenPacketDataV2.
    function abiPublicTypes(IICS20TransferMsgs.FungibleTokenPacketDataV2 memory o1) public pure 
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
    function newMsgSendPacketV2(
        address sender,
        IICS20TransferMsgs.SendTransferMsg memory msg_
    )
        external
        view
        returns (IICS26RouterMsgs.MsgSendPacket memory)
    {
        require(msg_.tokens.length > 0, IICS20Errors.ICS20InvalidAmount(msg_.tokens.length));

        IICS20TransferMsgs.Token[] memory tokens = new IICS20TransferMsgs.Token[](msg_.tokens.length);
        for (uint256 i = 0; i < msg_.tokens.length; i++) {
            require(msg_.tokens[i].amount > 0, IICS20Errors.ICS20InvalidAmount(msg_.tokens[i].amount));

            IICS20TransferMsgs.Denom memory fullDenom;
            try IBCERC20(msg_.tokens[i].contractAddress).fullDenom() returns (
                IICS20TransferMsgs.Denom memory fullDenomFromContract
            ) {
                // if the address is one of our IBCERC20 contracts, we get the correct denom for the packet there
                fullDenom = fullDenomFromContract;
            } catch {
                // otherwise this is just an ERC20 address, so we use it as the denom
                fullDenom = IICS20TransferMsgs.Denom({
                    base: Strings.toHexString(msg_.tokens[i].contractAddress),
                    trace: new IICS20TransferMsgs.Hop[](0)
                });
            }

            tokens[i] = IICS20TransferMsgs.Token({ denom: fullDenom, amount: msg_.tokens[i].amount });
        }

        string memory memo = msg_.memo;
        IICS20TransferMsgs.ForwardingPacketData memory forwarding =
            IICS20TransferMsgs.ForwardingPacketData({ destinationMemo: "", hops: msg_.forwarding.hops });
        if (msg_.forwarding.hops.length > 0) {
            memo = "";
            forwarding.destinationMemo = msg_.memo;
        }

        // We are encoding the payload in ABI format
        bytes memory packetData = abi.encode(
            IICS20TransferMsgs.FungibleTokenPacketDataV2({
                tokens: tokens,
                sender: Strings.toHexString(sender),
                receiver: msg_.receiver,
                memo: memo,
                forwarding: forwarding
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

    // TODO: Document
    function getPath(IICS20TransferMsgs.Denom memory denom) external pure returns (string memory) {
        if (denom.trace.length == 0) {
            return denom.base;
        }

        string memory trace = "";
        for (uint256 i = 0; i < denom.trace.length; i++) {
            if (i > 0) {
                trace = string(abi.encodePacked(trace, "/"));
            }
            trace = string(abi.encodePacked(trace, denom.trace[i].portId, "/", denom.trace[i].clientId));
        }

        return string(abi.encodePacked(trace, "/", denom.base));
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

    /// @notice errorAck returns an error acknowledgement.
    /// @param reason Error reason
    /// @return Error acknowledgement
    function errorAck(bytes memory reason) internal pure returns (bytes memory) {
        return abi.encodePacked("{\"error\":\"", reason, "\"}");
    }

    /// @notice hasPrefix checks if the denom is prefixed by the provided port and channel
    /// @param denom IICS20TransferMsgs.Denom to check for prefix
    /// @param port Port ID for the prefix
    /// @param client Client ID for the prefix
    function hasPrefix(
        IICS20TransferMsgs.Denom memory denom,
        string calldata port,
        string calldata client
    )
        internal
        pure
        returns (bool)
    {
        // if the denom is native, then it is not prefixed by any port/channel pair
        if (denom.trace.length == 0) {
            return false;
        }

        return denom.trace[0].portId.equal(port) && denom.trace[0].clientId.equal(client);
    }

    // TODO: Document
    function getDenomIdentifier(IICS20TransferMsgs.Denom memory denom) internal pure returns (bytes32) {
        bytes memory traceBytes = "";
        for (uint256 i = 0; i < denom.trace.length; i++) {
            traceBytes = abi.encodePacked(
                traceBytes, keccak256(abi.encodePacked(denom.trace[i].portId, "/", denom.trace[i].clientId))
            );
        }

        return keccak256(abi.encodePacked(denom.base, traceBytes));
    }
}
