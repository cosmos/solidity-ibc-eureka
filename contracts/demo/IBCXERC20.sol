// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IMintableAndBurnable } from "../interfaces/IMintableAndBurnable.sol";
import { IICS27GMPMsgs } from "../msgs/IICS27GMPMsgs.sol";

import { ERC20Upgradeable } from "@openzeppelin-upgradeable/token/ERC20/ERC20Upgradeable.sol";
import { UUPSUpgradeable } from "@openzeppelin-contracts/proxy/utils/UUPSUpgradeable.sol";
import { OwnableUpgradeable } from "@openzeppelin-upgradeable/access/OwnableUpgradeable.sol";
import { IICS27GMP } from "../interfaces/IICS27GMP.sol";

/// @title Reference IBC xERC20 Implementation
/// @notice This implementation is intended to serve as a base reference for developers creating their own
/// IBC-compatible upgradeable xERC20 tokens.
contract IBCXERC20 is IMintableAndBurnable, UUPSUpgradeable, ERC20Upgradeable, OwnableUpgradeable {
    /// @notice Caller is not authorized
    /// @param caller The address of the caller
    error CallerUnauthorized(address caller);

    /// @notice Storage of the IBCXERC20 contract
    /// @dev It's implemented on a custom ERC-7201 namespace to reduce the risk of storage collisions when using with
    /// upgradeable contracts.
    struct IBCXERC20Storage {
        IICS27GMP ics27Gmp;
        string clientId;
        string receiver;
        bytes payload;
    }

    /// @notice ERC-7201 slot for the IBCXERC20 storage
    /// @dev keccak256(abi.encode(uint256(keccak256("ibc.storage.IBCXERC20")) - 1)) & ~bytes32(uint256(0xff))
    bytes32 private constant IBCXERC20_STORAGE_SLOT = 0x3b29456b3eec312d403f3b5994cee4aa3b42a884561e0a5822147cf35e2b5a00;

    /// @dev This contract is meant to be deployed by a proxy, so the constructor is not used
    // natlint-disable-next-line MissingNotice
    constructor() {
        _disableInitializers();
    }

    /// @notice Initializes the RefIBCERC20 contract
    /// @param owner_ The owner of the contract, allowing it to be upgraded
    /// @param name_ The name of the token
    /// @param symbol_ The symbol of the token
    /// @param ics27Gmp_ The ICS27GMP contract address
    /// @param clientId_ The client ID on the source chain
    /// @param receiver_ The receiver address on the source chain
    /// @param payload_ The payload to be sent back on burn
    function initialize(
        address owner_,
        string calldata name_,
        string calldata symbol_,
        address ics27Gmp_,
        string calldata clientId_,
        string calldata receiver_,
        bytes calldata payload_
    )
        external
        initializer
    {
        __ERC20_init(name_, symbol_);
        __Ownable_init(owner_);

        IBCXERC20Storage storage $ = _getIBCXERC20Storage();
        $.ics27Gmp = IICS27GMP(ics27Gmp_);
        $.clientId = clientId_;
        $.receiver = receiver_;
        $.payload = payload_;
    }

    /**
     * @inheritdoc ERC20Upgradeable
     *
     * @dev Returns the number of decimals used to get its user representation.
     * For example, if `decimals` equals `2`, a balance of `505` tokens should
     * be displayed to a user as `5.05` (`505 / 10 ** 2`).
     *
     * Cosmos SDK tokens usually opt for a value of 6, imitating the relationship
     * between ATOM and uatom.
     *
     * NOTE: This information is only used for _display_ purposes such as by wallets:
     * it in no way affects any of the arithmetic of the contract, including
     * {IERC20-balanceOf} and {IERC20-transfer}.
     */
    function decimals() public pure override(ERC20Upgradeable) returns (uint8) {
        return 6;
    }

    /// @inheritdoc IMintableAndBurnable
    function mint(address mintAddress, uint256 amount) external onlyOwner {
        _mint(mintAddress, amount);
    }

    /// @inheritdoc IMintableAndBurnable
    function burn(address mintAddress, uint256 amount) external onlyOwner {
        _burn(mintAddress, amount);

        IBCXERC20Storage storage $ = _getIBCXERC20Storage();
        $.ics27Gmp.sendCall(
            IICS27GMPMsgs.SendCallMsg({
                sourceClient: $.clientId,
                receiver: $.receiver,
                salt: "",
                payload: $.payload,
                timeoutTimestamp: uint64(block.timestamp + 1 hours),
                memo: ""
            })
        );
    }

    /// @notice Returns the storage of the IBCXERC20 contract
    /// @return $ The storage of the IBCXERC20 contract
    function _getIBCXERC20Storage() private pure returns (IBCXERC20Storage storage $) {
        // solhint-disable-next-line no-inline-assembly
        assembly {
            $.slot := IBCXERC20_STORAGE_SLOT
        }
    }

    /// @inheritdoc UUPSUpgradeable
    function _authorizeUpgrade(address) internal view override(UUPSUpgradeable) onlyOwner { }
    // solhint-disable-previous-line no-empty-blocks
}
