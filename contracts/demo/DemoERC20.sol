// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IIBCERC20 } from "../interfaces/IIBCERC20.sol";
import { IMintableAndBurnable } from "../interfaces/IMintableAndBurnable.sol";
import { IIBCERC20Errors } from "../errors/IIBCERC20Errors.sol";
import { IICS27GMPMsgs } from "../msgs/IICS27GMPMsgs.sol";

import { XERC20 } from "@defi-wonderland/xerc20/contracts/XERC20.sol";
import { IICS27GMP } from "../interfaces/IICS27GMP.sol";

/// @title IBCXERC20 Contract
/// @notice This contract is the default xERC20 implementation for new IBC tokens.
/// @dev This is the default implementation to be deployed when new IBC tokens are received.
contract IBCXERC20 is XERC20 layout at 0 {
    IICS27GMP public immutable ICS27_GMP;
    bytes public payload;
    string public clientId;
    string public receiver;

    bytes constant COUNTERPARTY_MINT = "counterparty_mint";

    constructor(
        string memory name_,
        string memory symbol_,
        address factory_,
        address ics27_,
        bytes memory payload_,
        string memory clientId_,
        string memory receiver_
    )
        XERC20(name_, symbol_, factory_)
    {
        ICS27_GMP = IICS27GMP(ics27_);
        payload = payload_;
        clientId = clientId_;
        receiver = receiver_;
    }

    function burn(address _user, uint256 _amount) public override {
        super.burn(_user, _amount);
        ICS27_GMP.sendCall(
            IICS27GMPMsgs.SendCallMsg({
                sourceClient: clientId,
                receiver: receiver,
                salt: "",
                payload: payload,
                timeoutTimestamp: uint64(block.timestamp + 1 hours),
                memo: ""
            })
        );
    }
}
