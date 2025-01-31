// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import { IICS20Transfer } from "../../../contracts/interfaces/IICS20Transfer.sol";
import { IICS26RouterMsgs } from "../../../contracts/msgs/IICS26RouterMsgs.sol";
import { IICS20TransferMsgs } from "../../../contracts/msgs/IICS20TransferMsgs.sol";

contract DummyICS20Transfer is IICS20Transfer {
    // Dummy implementation of IICS20Transfer
    function sendTransfer(IICS20TransferMsgs.SendTransferMsg calldata) external pure returns (uint32 sequence) {
        return 0;
    }

    // Dummy implementation of IICS20Transfer
    function escrow() external pure override returns (address) {
        return address(0);
    }

    // Dummy implementation of IICS20Transfer
    function ibcERC20Contract(IICS20TransferMsgs.Denom calldata) external pure override returns (address) {
        return address(0);
    }

    // Dummy implementation of IICS20Transfer
    function newMsgSendPacketV2(
        address,
        IICS20TransferMsgs.SendTransferMsg calldata
    )
        external
        pure
        override
        returns (IICS26RouterMsgs.MsgSendPacket memory)
    {
        return IICS26RouterMsgs.MsgSendPacket("", 0, new IICS26RouterMsgs.Payload[](0));
    }
}
