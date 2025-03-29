// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IICS26RouterMsgs } from "../msgs/IICS26RouterMsgs.sol";
import { IICS20TransferMsgs } from "../msgs/IICS20TransferMsgs.sol";

import { IERC20 } from "@openzeppelin-contracts/token/ERC20/IERC20.sol";
import {IERC721} from "@openzeppelin/contracts/token/ERC721/IERC721.sol";
import {IERC721Metadata} from "@openzeppelin/contracts/token/ERC721/extensions/IERC721Metadata.sol";
import { SafeERC20 } from "@openzeppelin-contracts/token/ERC20/utils/SafeERC20.sol";
import { ERC721Upgradeable } from "@openzeppelin-upgradeable/token/ERC721/ERC721Upgradeable.sol";
import { AccessControlUpgradeable } from "@openzeppelin-upgradeable/access/AccessControlUpgradeable.sol";
import { ReentrancyGuardTransientUpgradeable } from
    "@openzeppelin-upgradeable/utils/ReentrancyGuardTransientUpgradeable.sol";


import { IICS26Router } from "../interfaces/IICS26Router.sol";
import { IICS20Transfer } from "../interfaces/IICS20Transfer.sol";
import { ICS20Lib } from "../utils/ICS20Lib.sol";

using SafeERC20 for IERC20;

contract IBCNFT721 is 
    ERC721Upgradeable,
    AccessControlUpgradeable,
    ReentrancyGuardTransientUpgradeable
{
     struct IBCNFT721Storage {
        IICS26Router _ics26;
        IICS20Transfer _ics20;
        address _admin;
        mapping(string denom => address user) _firstReceiver;
        mapping(string denom => uint256 amount) _vault;
    }

    bytes32 private constant IBCNFT721_STORAGE_SLOT = 0xed26987a4c0cd054c76bcc646d9bfb315b6bab9c405b583fff9fbb7c01979800;

    /// @dev This contract is meant to be deployed by a proxy, so the constructor is not used
    constructor() {
        _disableInitializers();
    }

    function initialize(
        address ics26Router,
        address ics20_,
        string memory name_,
        string memory symbol_
    ) 
        public
        initializer
    {
        __ReentrancyGuardTransient_init();
        __ERC721_init(name_, symbol_);

        IBCNFT721Storage storage $ = _getIBCNFT721Storage();
        $._admin = _msgSender();
        $._ics26 = IICS26Router(ics26Router);
        $._ics20 = IICS20Transfer(ics20_);
    }

    // if you call recvPacket through this function for the first time for a given denom
    // the receiver automatically gets authorized to mint the NFT for this denom
    function recvPacket(IICS26RouterMsgs.MsgRecvPacket calldata msg_) external nonReentrant {
        require(
            keccak256(bytes(msg_.packet.payloads[0].destPort)) == ICS20Lib.KECCAK256_DEFAULT_PORT_ID,
            "invalid port ID"
        );
        IICS20TransferMsgs.FungibleTokenPacketData memory packetData =
            abi.decode(msg_.packet.payloads[0].value, (IICS20TransferMsgs.FungibleTokenPacketData));
        IBCNFT721Storage storage $ = _getIBCNFT721Storage();

        // we are not origin source, i.e. sender chain is the origin source: add denom trace and mint vouchers
        bytes memory denomBz = bytes(packetData.denom);
        bytes memory newDenomPrefix = ICS20Lib.getDenomPrefix(msg_.packet.payloads[0].destPort, msg_.packet.destClient);
        bytes memory newDenom = abi.encodePacked(newDenomPrefix, denomBz);

        // unfortunate way to prove ERC20 contract already doesn't exist
        try $._ics20.ibcERC20Contract(string(newDenom)) {
            require(false, "denom already exists");
        } catch {}

        $._ics26.recvPacket(msg_);

        address newERC20 = $._ics20.ibcERC20Contract(string(newDenom));
        require(newERC20 != address(0), "denom was not created");

        // authorize receiver to claim the NFT for this denom
        address receiver = ICS20Lib.mustHexStringToAddress(packetData.receiver);
        _authorize(string(newDenom), receiver);
    }

    // we also allow the admin to manually authorize adding the first sender
    // of a newdenom if they did not use the automatic endpoint
    function authorize(string memory denom, address user) external onlyAdmin {
        IBCNFT721Storage storage $ = _getIBCNFT721Storage();
        require($._firstReceiver[denom] == address(0), "already authorized");
        $._firstReceiver[denom] = user;
    }

    // this sets the user as the only address that can redeem the NFT for 
    function _authorize(string memory denom, address user) internal {
        IBCNFT721Storage storage $ = _getIBCNFT721Storage();
        require($._firstReceiver[denom] == address(0), "already authorized");
        $._firstReceiver[denom] = user;
    }

    function claim(string memory denom, uint256 amount) external {
        IBCNFT721Storage storage $ = _getIBCNFT721Storage();
        require($._firstReceiver[denom] == _msgSender(), "not authorized");
        address erc20 = $._ics20.ibcERC20Contract(denom);
        IERC20(erc20).safeTransferFrom(_msgSender(), address(this), amount);
        $._vault[denom] = amount;
        _safeMint(_msgSender(), uint256((keccak256(bytes(denom)))));
    }

    function destroy(string memory denom) external {
        IBCNFT721Storage storage $ = _getIBCNFT721Storage();
        require(ownerOf(uint256(keccak256(bytes(denom)))) == _msgSender(), "non owner cannot destroy NFT");
        // send back the vault tokens
        address erc20 = $._ics20.ibcERC20Contract(denom);
        IERC20(erc20).safeTransfer(_msgSender(), $._vault[denom]);
        _burn(uint256(keccak256(bytes(denom))));
    }

    function whoIsAuthorized(string memory denom) external view returns (address) {
        IBCNFT721Storage storage $ = _getIBCNFT721Storage();
        return $._firstReceiver[denom];
    }

    /// @notice Returns the storage of the IBCNFT721 contract
    function _getIBCNFT721Storage() private pure returns (IBCNFT721Storage storage $) {
        // solhint-disable-next-line no-inline-assembly
        assembly {
            $.slot := IBCNFT721_STORAGE_SLOT
        }
    }

    function ics26() external view returns (address) {
        return address(_getIBCNFT721Storage()._ics26);
    }

    function ics20() external view returns (address) {
        return address(_getIBCNFT721Storage()._ics20);
    }

    modifier onlyAdmin() {
        IBCNFT721Storage storage $ = _getIBCNFT721Storage();
        require(_msgSender() == $._admin, "only admin can call this function");
        _;
    }

     /**
     * @dev See {IERC165-supportsInterface}.
     */
    function supportsInterface(bytes4 interfaceId) public view virtual override(ERC721Upgradeable, AccessControlUpgradeable) returns (bool) {
        return
            interfaceId == type(IERC721).interfaceId ||
            interfaceId == type(IERC721Metadata).interfaceId ||
            super.supportsInterface(interfaceId);
    }


}