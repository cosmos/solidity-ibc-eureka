// SPDX-License-Identifier: MIT
pragma solidity >=0.8.25;

import { IICS20Transfer } from "../interfaces/IICS20Transfer.sol";
import { Ownable } from "@openzeppelin/contracts/access/Ownable.sol";

contract ICS20Escrow is Ownable {
    /// @param owner_ The owner of the contract
    constructor(IICS20Transfer owner_) Ownable(address(owner_)) { }
}
