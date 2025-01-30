// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import { IICS20Transfer } from "../../../contracts/interfaces/IICS20Transfer.sol";
import { ICS20Lib } from "../../../contracts/utils/ICS20Lib.sol";
import { IICS26RouterMsgs } from "../../../contracts/msgs/IICS26RouterMsgs.sol";

contract DummyICS20Transfer is IICS20Transfer {
    // Dummy implementation of IICS20Transfer
    function sendTransfer(SendTransferMsg calldata) external pure returns (uint32 sequence) {
        return 0;
    }

    // Dummy implementation of IICS20Transfer
    function escrow() external pure override returns (address) {
        return address(0);
    }

    // Dummy implementation of IICS20Transfer
    function ibcERC20Contract(ICS20Lib.Denom calldata) external pure override returns (address) {
        return address(0);
    }

    // Dummy implementation of IICS20Transfer
    function newMsgSendPacketV2(
        address,
        SendTransferMsg calldata
    )
        external
        pure
        override
        returns (IICS26RouterMsgs.MsgSendPacket memory)
    {
        return IICS26RouterMsgs.MsgSendPacket("", 0, new IICS26RouterMsgs.Payload[](0));
    }
}
