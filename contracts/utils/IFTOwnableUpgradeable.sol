// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IFTBaseUpgradeable } from "./IFTBaseUpgradeable.sol";
import { OwnableUpgradeable } from "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";

contract IFTOwnable is IFTBaseUpgradeable, OwnableUpgradeable {

}
