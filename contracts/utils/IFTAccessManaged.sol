// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IFTBaseUpgradeable } from "./IFTBaseUpgradeable.sol";
import { AccessManagedUpgradeable } from "@openzeppelin-upgradeable/access/manager/AccessManagedUpgradeable.sol";
import { UUPSUpgradeable } from "@openzeppelin-contracts/proxy/utils/UUPSUpgradeable.sol";
import { ERC20Upgradeable } from "@openzeppelin-upgradeable/token/ERC20/ERC20Upgradeable.sol";

/// @title IFT Access Managed
/// @notice This is the access managed and upgradable implementation of IFT
/// @dev If you need a custom IFT implementation, then inherit from IFTBaseUpgradeable instead of deploying this
/// contract directly @dev WARNING: This contract is experimental
contract IFTAccessManaged is IFTBaseUpgradeable, AccessManagedUpgradeable, UUPSUpgradeable {
    // natlint-disable-next-line MissingNotice
    constructor() {
        _disableInitializers();
    }

    /// @notice Initializes the IFTAccessManaged contract
    /// @param authority_ The address of the AccessManager contract
    /// @param erc20Name The name of the ERC20 token
    /// @param erc20Symbol The symbol of the ERC20 token
    /// @param ics27Gmp The address of the ICS27-GMP contract
    // natlint-disable-next-line MissingInheritdoc
    function initialize(
        address authority_,
        string calldata erc20Name,
        string calldata erc20Symbol,
        address ics27Gmp
    )
        external
        initializer
    {
        __AccessManaged_init(authority_);
        __IFTBase_init(erc20Name, erc20Symbol, ics27Gmp);
    }

    /// @inheritdoc IFTBaseUpgradeable
    function _onlyAuthority() internal override(IFTBaseUpgradeable) restricted { }
    // solhint-disable-previous-line no-empty-blocks

    /// @inheritdoc UUPSUpgradeable
    function _authorizeUpgrade(address) internal override(UUPSUpgradeable) restricted { }
    // solhint-disable-previous-line no-empty-blocks

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
}
