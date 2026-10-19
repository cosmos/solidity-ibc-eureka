// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.28;

import { IIBCMulticallMsgs } from "../msgs/IIBCMulticallMsgs.sol";

import { IIBCMulticall } from "../interfaces/IIBCMulticall.sol";

import { AccessManagedUpgradeable } from "@openzeppelin-upgradeable/access/manager/AccessManagedUpgradeable.sol";
import { UUPSUpgradeable } from "@openzeppelin-contracts/proxy/utils/UUPSUpgradeable.sol";
import { LowLevelCall } from "@openzeppelin-contracts/utils/LowLevelCall.sol";

/// @title IBC Multicall
/// @notice Lets relayers batch packet relays where a failing relay does not revert the rest of the batch
/// @dev Map `multicall` to `RELAYER_ROLE` and grant this contract `RELAYER_ROLE`, so only relayers can call it and it
/// can relay to the router. See `DeployAccessManagerWithRoles.accessManagerSetMulticallRoles`.
/// @dev The router reports this contract as the relayer to IBC applications.
contract IBCMulticall is IIBCMulticall, AccessManagedUpgradeable, UUPSUpgradeable {
    // natlint-disable-next-line MissingNotice
    constructor() {
        _disableInitializers();
    }

    /// @notice Initializes the contract
    /// @param authority The address of the AccessManager contract
    // natlint-disable-next-line MissingInheritdoc
    function initialize(address authority) external initializer {
        __AccessManaged_init(authority);
    }

    /// @inheritdoc IIBCMulticall
    function multicall(IIBCMulticallMsgs.Call[] calldata calls)
        external
        restricted
        returns (IIBCMulticallMsgs.Result[] memory results)
    {
        results = new IIBCMulticallMsgs.Result[](calls.length);
        for (uint256 i = 0; i < calls.length; ++i) {
            IIBCMulticallMsgs.Call calldata call = calls[i];
            // solhint-disable-next-line avoid-low-level-calls
            (bool success, bytes memory returnData) = call.target.call(call.callData);
            if (!success) {
                if (!call.allowFailure) {
                    LowLevelCall.bubbleRevert(returnData);
                }
                emit CallFailed(i, returnData);
            }
            results[i] = IIBCMulticallMsgs.Result({ success: success, returnData: returnData });
        }
    }

    /// @inheritdoc UUPSUpgradeable
    function _authorizeUpgrade(address) internal override restricted { }
    // solhint-disable-previous-line no-empty-blocks
}
