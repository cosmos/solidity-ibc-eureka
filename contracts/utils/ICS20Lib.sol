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
// It has since been modified heavily (e.g. ICS20-2, replacing JSON with ABI encoding, adding new functions, etc.)
library ICS20Lib {
    using Strings for string;

    /// @notice FungibleTokenPacketData is the payload for a fungible token transfer packet.
    /// @dev See FungibleTokenPacketDataV2 spec:
    /// https://github.com/cosmos/ibc/tree/master/spec/app/ics-020-fungible-token-transfer#data-structures
    /// @param tokens The tokens to be transferred
    /// @param sender The sender of the token
    /// @param receiver The receiver of the token
    /// @param memo Optional memo
    /// @param forwarding Optional forwarding information
    struct FungibleTokenPacketData {
        Token[] tokens;
        string sender;
        string receiver;
        string memo;
        ForwardingPacketData forwarding;
    }

    /// @notice ForwardingPacketData defines a list of port ID, channel ID pairs determining the path
    /// through which a packet must be forwarded, and the destination memo string to be used in the
    /// final destination of the tokens.
    /// @param destination_memo Optional memo consumed by final destination chain
    /// @param hops Optional intermediate path through which packet will be forwarded.
    struct ForwardingPacketData {
        string destinationMemo;
        Hop[] hops;
    }

    /// @notice Token holds the denomination and amount of a token to be transferred.
    /// @param denom The token denomination
    /// @param amount The token amount
    struct Token {
        Denom denom;
        uint256 amount;
    }

    /// @notice Denom holds the base denom of a Token and a trace of the chains it was sent through.
    /// @param base The base token denomination
    /// @param trace The trace of the token
    struct Denom {
        string base;
        Hop[] trace;
    }    

    /// @notice Hop defines a port ID, channel ID pair specifying where tokens must be forwarded
    /// next in a multihop transfer, or the trace of an existing token.
    /// @param portId The port ID
    /// @param channelId The channel ID
    struct Hop {
        string portId;
        string channelId;
    }

    /// @notice ICS20_VERSION is the version string for ICS20 packet data.
    string public constant ICS20_VERSION = "ics20-2";

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
    function newMsgSendPacketV2(
        address sender,
        IICS20TransferMsgs.SendTransferMsg memory msg_
    )
        external
        view
        returns (IICS26RouterMsgs.MsgSendPacket memory)
    {
        

        Token[] memory tokens = new Token[](msg_.tokens.length);
        for (uint256 i = 0; i < msg_.tokens.length; i++) {
            require(msg_.tokens[i].amount > 0, IICS20Errors.ICS20InvalidAmount(msg_.tokens[i].amount));

            Denom memory fullDenom;
            // TODO: Is this correct?
            try IBCERC20(mustHexStringToAddress(msg_.tokens[i].denom.base)).fullDenom() returns (Denom memory fullDenomFromContract) {
                // if the address is one of our IBCERC20 contracts, we get the correct denom for the packet there
                fullDenom = fullDenomFromContract;
            } catch {
                // otherwise this is just an ERC20 address, so we use it as the denom
                fullDenom = msg_.tokens[i].denom;
            }

            tokens[i] = Token({
                denom: fullDenom,
                amount: msg_.tokens[i].amount
            });
        }

        ForwardingPacketData memory forwarding = ForwardingPacketData({
            destinationMemo: msg_.memo,
            hops: msg_.forwarding.hops
        });
        

        // We are encoding the payload in ABI format
        bytes memory packetData = abi.encode(
            ICS20Lib.FungibleTokenPacketData({
                tokens: msg_.tokens,
                sender: Strings.toHexString(sender),
                receiver: msg_.receiver,
                memo: msg_.memo,
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
            sourceChannel: msg_.sourceClient,
            timeoutTimestamp: msg_.timeoutTimestamp,
            payloads: payloads
        });
    }

    // TODO: FIX THESE TO AVOID THE SWAPPED ORDER OF ARGUMENTS (AND REMOVE ONE OF THESE PROBABLY)
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

    /// @notice errorAck returns an error acknowledgement.
    /// @param reason Error reason
    /// @return Error acknowledgement
    function errorAck(bytes memory reason) internal pure returns (bytes memory) {
        return abi.encodePacked("{\"error\":\"", reason, "\"}");
    }

    /// @notice hasPrefix checks if the denom is prefixed by the provided port and channel
    /// @param denom Denom to check for prefix
    /// @param port Port ID for the prefix
    /// @param channel Channel ID for the prefix
    function hasPrefix(Denom memory denom, string calldata port, string calldata channel) internal pure returns (bool) {
        // if the denom is native, then it is not prefixed by any port/channel pair
       if (denom.trace.length == 0) {
           return false;
       }

       return denom.trace[0].portId.equal(port) && denom.trace[0].channelId.equal(channel);
    }

    function getDenomIdentifier(Denom memory denom) internal pure returns (bytes32) {
        bytes memory traceBytes = "";
        for (uint256 i = 0; i < denom.trace.length; i++) {
            traceBytes = abi.encodePacked(traceBytes, keccak256(abi.encodePacked(denom.trace[i].portId, denom.trace[i].channelId)));
        }

        return keccak256(abi.encodePacked(denom.base, traceBytes));
    }

    // /// @notice toIBCDenom converts a full denom path to an ibc/hash(trace+base_denom) denom
    // /// @notice there is no check if the denom passed in is a base denom (if it has no trace), so it is assumed
    // /// @notice that the denom passed in is a full denom path with trace and base denom
    // /// @param fullDenomPath full denom path with trace and base denom
    // /// @return IBC denom in the format ibc/hash(trace+base_denom)
    // function toIBCDenom(string memory fullDenomPath) public pure returns (string memory) {
    //     string memory hash = toHexHash(fullDenomPath);
    //     return string(abi.encodePacked(IBC_DENOM_PREFIX, hash));
    // }

    // TODO: IS THIS USED ANYWHERE?
    /// @notice toHexHash converts a string to an all uppercase hex hash (without the 0x prefix)
    /// @param str string to convert
    /// @return uppercase hex hash without 0x prefix
    function toHexHash(string memory str) public pure returns (string memory) {
        bytes32 hash = sha256(bytes(str));
        bytes memory hexBz = bytes(Strings.toHexString(uint256(hash)));

        // next we remove the `0x` prefix and uppercase the hash string
        bytes memory finalHex = new bytes(hexBz.length - 2); // we skip the 0x prefix

        for (uint256 i = 2; i < hexBz.length; i++) {
            // if lowercase a-z, convert to uppercase
            if (hexBz[i] >= 0x61 && hexBz[i] <= 0x7A) {
                finalHex[i - 2] = bytes1(uint8(hexBz[i]) - 32);
            } else {
                finalHex[i - 2] = hexBz[i];
            }
        }

        return string(finalHex);
    }
}
