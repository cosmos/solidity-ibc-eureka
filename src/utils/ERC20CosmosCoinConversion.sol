// SPDX-License-Identifier: MIT
pragma solidity >=0.8.25;

import { Strings } from "@openzeppelin/contracts/utils/Strings.sol";

library ERC20CosmosCoinConversion {
    //using SafeERC20 for IERC20;
    // Using Constants for decimals
    // TODO In case we want to support flexibility we have to change these constant.
    // Note that ERC20 standard use 18 decimals by default. Custom ERC20 implementation may decide to change this. 
    // Potential problems that may arise: https://detectors.auditbase.com/decimals-erc20-standard-solidity
    uint8 constant ERC20_DECIMALS = 18; 
    // https://docs.cosmos.network/v0.50/build/architecture/adr-024-coin-metadata  
    uint8 constant COSMOS_DECIMALS = 6;

    // Convert ERC20 tokens decimals to Cosmos decimals units  
    // NOTE that the refund of the remainder of the conversion should happen during the transfer.  
    function _convertERC20AmountToCosmosCoin(
        uint256 amount
    ) internal pure returns (uint256 convertedAmount, uint256 remainder) {
        // TODO if we want to allow different decimals, we need to handle that here 
        uint256 factor = 10 ** (ERC20_DECIMALS - COSMOS_DECIMALS);
        convertedAmount = amount / factor;
        remainder = amount % factor;

        return (convertedAmount, remainder);
    }

    // Convert Cosmos coin amount to ERC20 token amount
    function _convertCosmosCoinAmountToERC20(
        uint256 amount
    ) internal pure returns (uint256) {
        uint256 factor = 10 ** (ERC20_DECIMALS - COSMOS_DECIMALS);
        return amount * factor;
    }

    // Convert ERC20 token name to Cosmos coin name
    function _convertERC20NameToCosmosCoin(
        string memory name, 
        string memory channel
    ) internal pure returns (string memory) {
        // TODO check About string(abi.encodePacked https://medium.com/coinmonks/abi-encode-abi-encodepacked-and-abi-decode-in-solidity-42c19336a589
        // vs string.concat 
        return string(abi.encodePacked(name, " channel-", channel));
    }

    // Convert ERC20 token symbol to Cosmos coin symbol
    function _convertERC20SymbolToCosmosCoin(
        string memory symbol, 
        string memory channel
    ) internal pure returns (string memory) {
        return string(abi.encodePacked("ibc", symbol, "-", channel));
    }

    // Convert Cosmos coin metadata to ERC20 token details
    function _convertCosmosCoinToERC20Details(
        string memory name,
        string memory symbol,
        uint8 decimals
    ) internal pure returns (string memory, string memory, uint8) {
        return (name, symbol, decimals);
    }

    // Convert ERC20 token details to Cosmos coin metadata
    function _convertERC20ToCosmosCoinMetadata(
        string memory name,
        string memory symbol,
        uint8 decimals,
        address contractAddress
    ) internal pure returns (
        string memory description,
        uint32 denomUnitsCoin,
        uint32 denomUnitsERC20,
        string memory base,
        string memory display,
        string memory coinName,
        string memory coinSymbol
    ) {
        description = string.concat("Cosmos coin token representation of ", Strings.toHexString(contractAddress));
        denomUnitsCoin = 6;
        denomUnitsERC20 = uint32(decimals);

        base = string.concat("erc20/", Strings.toHexString(contractAddress));
        display = name;
        coinName = name;
        coinSymbol = symbol;
        return (description, denomUnitsCoin, denomUnitsERC20, base, display, coinName, coinSymbol);
    }

}