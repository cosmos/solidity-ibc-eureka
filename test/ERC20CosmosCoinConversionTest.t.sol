// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25 <0.9.0;

// solhint-disable custom-errors,max-line-length

import { Test } from "forge-std/Test.sol";
import { ERC20CosmosCoinConversion } from "../src/utils/ERC20CosmosCoinConversion.sol";
import { Strings } from "@openzeppelin/contracts/utils/Strings.sol";

contract ERC20CosmosCoinConversionTest is Test {
    using ERC20CosmosCoinConversion for uint256;

    function testConvertERC20AmountToCosmosCoin() pure public {
        (uint256 convertedAmount, uint256 remainder) = ERC20CosmosCoinConversion._convertERC20AmountToCosmosCoin(1000000000000000000); // 1 ERC20 token with 18 decimals
        uint256 expectedConvertedAmount = 1000000; // 1 ERC20 token should convert to 1,000,000 Cosmos coins
        uint256 expectedRemainder = 0;

        assertEq(convertedAmount, expectedConvertedAmount);
        assertEq(remainder, expectedRemainder);
    }

    function testConvertCosmosCoinAmountToERC20() pure public {
        uint256 convertedAmount = ERC20CosmosCoinConversion._convertCosmosCoinAmountToERC20(1000000); // 1,000,000 Cosmos coins
        uint256 expectedConvertedAmount = 1000000000000000000; // Should convert to 1 ERC20 token

        assertEq(convertedAmount, expectedConvertedAmount);
    }

    function testConvertERC20NameToCosmosCoin() pure public {
        string memory name = "Token";
        string memory channel = "123";
        string memory convertedName = ERC20CosmosCoinConversion._convertERC20NameToCosmosCoin(name, channel);
        string memory expectedName = "Token channel-123";

        assertEq(convertedName, expectedName);
    }

    function testConvertERC20SymbolToCosmosCoin() pure public {
        string memory symbol = "TKN";
        string memory channel = "123";
        string memory convertedSymbol = ERC20CosmosCoinConversion._convertERC20SymbolToCosmosCoin(symbol, channel);
        string memory expectedSymbol = "ibcTKN-123";

        assertEq(convertedSymbol, expectedSymbol);
    }

    function testConvertCosmosCoinToERC20Details() pure public {
        string memory name = "Token";
        string memory symbol = "TKN";
        uint8 decimals = 18;

        (string memory erc20Name, string memory erc20Symbol, uint8 erc20Decimals) = ERC20CosmosCoinConversion._convertCosmosCoinToERC20Details(name, symbol, decimals);

        assertEq(erc20Name, name);
        assertEq(erc20Symbol, symbol);
        assertEq(erc20Decimals, decimals);
    }

    function testConvertERC20ToCosmosCoinMetadata() view public {
        string memory name = "Token";
        string memory symbol = "TKN";
        uint8 decimals = 18;
        address contractAddress = address(this);

        (
            string memory description,
            uint32 denomUnitsCoin,
            uint32 denomUnitsERC20,
            string memory base,
            string memory display,
            string memory coinName,
            string memory coinSymbol
        ) = ERC20CosmosCoinConversion._convertERC20ToCosmosCoinMetadata(name, symbol, decimals, contractAddress);

        string memory expectedDescription = string.concat("Cosmos coin token representation of ", Strings.toHexString(contractAddress));
        string memory expectedBase = string.concat("erc20/", Strings.toHexString(contractAddress));

        assertEq(description, expectedDescription);
        assertEq(denomUnitsCoin, 6);
        assertEq(denomUnitsERC20, decimals);
        assertEq(base, expectedBase);
        assertEq(display, name);
        assertEq(coinName, name);
        assertEq(coinSymbol, symbol);
    }
}
