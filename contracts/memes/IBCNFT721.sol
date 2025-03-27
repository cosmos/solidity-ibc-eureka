// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IICS26RouterMsgs } from "./msgs/IICS26RouterMsgs.sol";
import { IICS20TransferMsgs } from "./msgs/IICS20TransferMsgs.sol";

import { IERC20 } from "@openzeppelin-contracts/token/ERC20/IERC20.sol";
import { ERC721Upgradable } from "@openzeppelin/contracts-upgradeable/token/ERC721/ERC721Upgradable.sol";
import { AccessControlUpgradeable } from "@openzeppelin/contracts-upgradeable/access/AccessControlUpgradeable.sol";
import { ReentrancyGuardTransientUpgradeable } from
    "@openzeppelin-upgradeable/utils/ReentrancyGuardTransientUpgradeable.sol";


import { IICS26Router } from "../interfaces/IICS26Router.sol";
import { IICS20Transfer } from "../interfaces/IICS20Transfer.sol";
import { ICS20Lib } from "../utils/ICS20Lib.sol";

using SafeERC20 for IERC20;

contract IBCNFT721 is ERC721Upgradable, AccessControlUpgradeable {
     struct IBCERC721Storage {
        IICS26Router _ics26;
        IICS20Transfer _ics20;
        address _admin;
        mapping(address denom => address user) private _firstSender;
        mapping(address denom => uint256 amount) private _vault;
    }

    // if you call recvPacket through this function for the first time for a given denom
    // the receiver automatically gets authorized to mint the NFT for this denom
    function recvPacket(IICS26RouterMsgs.MsgRecvPacket calldata msg_) external nonReentrant {
        require(
            keccak256(msg_.packet.payloads.destPort) == ICS20Lib.KECCAK256_DEFAULT_PORT_ID,
            "invalid port ID"
        );
        IICS20TransferMsgs.FungibleTokenPacketData memory packetData =
            abi.decode(msg_.payload.value, (IICS20TransferMsgs.FungibleTokenPacketData));

        // we are not origin source, i.e. sender chain is the origin source: add denom trace and mint vouchers
        bytes memory newDenomPrefix = ICS20Lib.getDenomPrefix(msg_.payload.destPort, msg_.destinationClient);
        bytes memory newDenom = abi.encodePacked(newDenomPrefix, denomBz);

        exists = _ics20.ibcERC20Contract(newDenom)
        require(exists == address(0), "denom already exists");
        // should never happen but this is a safety check
        require(_firstSender[exists] == address(0), "tokenId already reserved")

        _ics26.recvPacket(msg_);

        exists = _ics20.ibcERC20Contract(newDenom)
        require(exists != address(0), "denom was not created");

        address receiver = ICS20Lib.mustHexStringToAddress(packetData.receiver);
        _authorize(tokenId, receiver);

    }

    // we also allow the admin to manually authorize adding the first sender
    // of a newdenom if they did not use the automatic endpoint
    function authorize(address denom, address user) external onlyAdmin {
        _firstSender[denom] = user;
    }

    // this sets the user as the only address that can redeem the NFT for 
    function _authorize(address denom, address user) internal {
        _firstSender[denom] = user;
    }

    function redeem(address denom, amount uint256) external {
        require(_firstSender[denom] == msg.sender, "not authorized");
        IERC20(denom).safeTransferFrom(msg.sender, address(this), amount);
        _vault[denom] = amount;
        _safeMint(msg.sender, uint256(denom));
    }

    function destroy(address denom) external {
        require(_owners[uint256(denom)] == msg.sender, "non owner cannot destroy NFT");
        // send back the vault tokens
        IERC20(denom).safeTransfer(msg.sender, _vault[denom]);
        _burn(uint256(denom));
    }

    modifier onlyAdmin() {
        require(msg.sender == _admin, "only admin");
        _;
    }

}