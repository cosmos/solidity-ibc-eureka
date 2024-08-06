// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25 <0.9.0;

// solhint-disable custom-errors,max-line-length
import { Test } from "forge-std/Test.sol";
import { ERC20CosmosCoinConversion } from "../src/utils/ERC20CosmosCoinConversion.sol";
import { Strings } from "@openzeppelin/contracts/utils/Strings.sol";
import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import {IERC20Metadata} from "@openzeppelin/contracts/token/ERC20/extensions/IERC20Metadata.sol";

// Discuss - Do we want to move mock contract to a mock folder?
// Mock ERC20 token without the decimals function overridden
contract MockERC20 is ERC20 {
    constructor() ERC20("BaseToken", "BTK") {
        _mint(msg.sender, 100000000000000000000); // Mint some tokens for testing
    }
}

// Discuss - Do we want to move mock contract to a mock folder?
// Mock ERC20 token with the decimals function overridden
contract MockERC20Metadata is ERC20 {
    uint8 private _decimals;

    constructor(uint8 decimals_) ERC20("MetadataToken", "MTK") {
        _decimals = decimals_;
        _mint(msg.sender, 100000000000000000000); // Mint some tokens for testing
    }

    function decimals() public view override returns (uint8) {
        return _decimals;
    }
}

contract ERC20CosmosCoinConversionTest is Test {
    
    // Instance of the MockERC20 contract
    MockERC20 private mockERC20;
    MockERC20Metadata private mockERC20Metadata;

    function setUp() public {
        // Deploy the mock ERC20 tokens
        mockERC20 = new MockERC20();
        mockERC20Metadata = new MockERC20Metadata(18);
    }

// Tests for MockERC20 - Standard Tokens implementation without decimals --> decimals = 18 

/**
    Problem Breakdown 
    ERC20 to Cosmos Coin Conversion:
    We are converting an ERC20 token amount to a Cosmos coin amount.
    - ERC20 standard use 18 decimals.
    - Cosmos coins use 6 decimals. 
    Example Conversion:
    - Input: 1000000000000000001 (1 ERC20 token + 1 extra smallest unit, i.e., 1 Wei).
    - Expected Converted Amount: 1000000 (1 ERC20 token should convert to 1,000,000 Cosmos coins).
    - Expected Remainder: 1 (which is the remaining smallest unit that doesn't fit into the Cosmos coin format).
    The Remainder is what we espect to return to the user 
    Detailed Explanation:
    - Given 1000000000000000001 as input: 
    Conversion Calculation:
    - 1 ERC20 token = 1000000000000000000 (in Wei).
    - Converting to Cosmos coin units involves dividing by 10^12 
    (since ERC20 has 18 decimals and Cosmos has 6 decimals, the difference is 18 - 6 = 12). 
    Mathematical Conversion:
    - Converted Amount: 1000000000000000001 / 10^12 = 1000000 (Cosmos coins).
    - Remainder: 1000000000000000001 % 10^12 = 1 (Wei remaining).
     */

    function testConvertMockERC20TokenAmountToCosmosCoin_1() public view {
        uint256 amount = 1000000000000000001; // 1.000000000000000001 ERC20 tokens
        // Call the conversion function
        (uint64 convertedAmount, uint256 remainder) = ERC20CosmosCoinConversion._convertERC20AmountToCosmosCoin(address(mockERC20), amount);

        // Expected values for default 18 decimals
        uint64 expectedConvertedAmount = 1000000; // 1,000,000 Cosmos coins
        uint256 expectedRemainder = 1;

        // Assertions
        assertEq(convertedAmount, expectedConvertedAmount, "Converted amount mismatch");
        assertEq(remainder, expectedRemainder, "Remainder mismatch");
    }


    function testConvertMockERC20AmountToCosmosCoin_2() public view {
        uint256 amount = 1000000100000000001; // 1.000000100000000001 ERC20 tokens
        // Call the conversion function
        (uint64 convertedAmount, uint256 remainder) = ERC20CosmosCoinConversion._convertERC20AmountToCosmosCoin(address(mockERC20), amount);

        // Expected values for default 18 decimals
        uint64 expectedConvertedAmount = 1000000; // 1,000,000 Cosmos coins
        uint256 expectedRemainder = 100000000001;

        // Assertions
        assertEq(convertedAmount, expectedConvertedAmount, "Converted amount mismatch");
        assertEq(remainder, expectedRemainder, "Remainder mismatch");
    }

    function testConvertMockERC20AmountToCosmosCoin_3() public view {
        uint256 amount = 1000000000000000000; // 1.000000000000000000 ERC20 tokens
        // Call the conversion function
        (uint64 convertedAmount, uint256 remainder) = ERC20CosmosCoinConversion._convertERC20AmountToCosmosCoin(address(mockERC20), amount);

        // Expected values for default 18 decimals
        uint64 expectedConvertedAmount = 1000000; // 1,000,000 Cosmos coins
        uint256 expectedRemainder = 0;

        // Assertions
        assertEq(convertedAmount, expectedConvertedAmount, "Converted amount mismatch");
        assertEq(remainder, expectedRemainder, "Remainder mismatch");
    }

    function testConvertMockERC20AmountToCosmosCoin_4() public view {
        uint256 amount = 1; // 1 smallest unit of ERC20 tokens
        // Call the conversion function
        (uint64 convertedAmount, uint256 remainder) = ERC20CosmosCoinConversion._convertERC20AmountToCosmosCoin(address(mockERC20), amount);

        // Expected values for default 18 decimals
        uint64 expectedConvertedAmount = 0;
        uint256 expectedRemainder = 1;

        // Assertions
        assertEq(convertedAmount, expectedConvertedAmount, "Converted amount mismatch");
        assertEq(remainder, expectedRemainder, "Remainder mismatch");
    }

    function testConvertMockERC20AmountToCosmosCoin_5() public view {
        uint256 amount = 999999999999; // Less than 1 smallest unit of Cosmos coins
        // Call the conversion function
        (uint64 convertedAmount, uint256 remainder) = ERC20CosmosCoinConversion._convertERC20AmountToCosmosCoin(address(mockERC20), amount);

        // Expected values for default 18 decimals
        uint64 expectedConvertedAmount = 0;
        uint256 expectedRemainder = 999999999999;

        // Assertions
        assertEq(convertedAmount, expectedConvertedAmount, "Converted amount mismatch");
        assertEq(remainder, expectedRemainder, "Remainder mismatch");
    }

    function testConvertMockERC20AmountToCosmosCoin_6() public view {
        uint256 amount = 999999999999999999; 
        // Call the conversion function
        (uint64 convertedAmount, uint256 remainder) = ERC20CosmosCoinConversion._convertERC20AmountToCosmosCoin(address(mockERC20), amount);

        // Expected values for default 18 decimals
        uint64 expectedConvertedAmount = 999999;
        uint256 expectedRemainder = 999999999999;

        // Assertions
        assertEq(convertedAmount, expectedConvertedAmount, "Converted amount mismatch");
        assertEq(remainder, expectedRemainder, "Remainder mismatch");
    }
    
    // Tests for MockERC20Metadata - Extended Tokens Standard implementation with decimals 
    function testConvertMockERC20MetadataAmountToCosmosCoin() public {
        uint8 decimals = 18; 
        // Deploy the ERC20 token with metadata (custom decimals)
        MockERC20Metadata customMockERC20Metadata = new MockERC20Metadata(decimals);
        uint256 amount = 1000000000000000001; // 1.000000000000000001 ERC20 tokens

        // Call the conversion function
        (uint64 convertedAmount, uint256 remainder) = ERC20CosmosCoinConversion._convertERC20AmountToCosmosCoin(address(customMockERC20Metadata), amount);

        // Expected values for 18 decimals
        uint64 expectedConvertedAmount = 1000000; // 1,000,000 Cosmos coins
        uint256 expectedRemainder = 1;

        // Assertions
        assertEq(convertedAmount, expectedConvertedAmount, "Converted amount mismatch");
        assertEq(remainder, expectedRemainder, "Remainder mismatch");
    }

    function testConvertMockERC20MetadataAmountToCosmosCoin_1() public {
        uint8 decimals = 77; 
        // Deploy the ERC20 token with metadata (custom decimals)
        MockERC20Metadata customMockERC20Metadata = new MockERC20Metadata(decimals);
        uint256 amount = 100000010000000000000000000000000000000000000000000000000000000000000000000001; 
        // Call the conversion function
        (uint64 convertedAmount, uint256 remainder) = ERC20CosmosCoinConversion._convertERC20AmountToCosmosCoin(address(customMockERC20Metadata), amount);

        // Expected values for 18 decimals
        uint64 expectedConvertedAmount = 1000000; // 1,000,000 Cosmos coins
        uint256 expectedRemainder = 10000000000000000000000000000000000000000000000000000000000000000000001;

        // Assertions
        assertEq(convertedAmount, expectedConvertedAmount, "Converted amount mismatch");
        assertEq(remainder, expectedRemainder, "Remainder mismatch");
    }


    function testConvertMockERC20MetadataAmountToCosmosCoin_2() public  {
        uint8 decimals = 17; 
        // Deploy the ERC20 token with metadata (custom decimals)
        MockERC20Metadata customMockERC20Metadata = new MockERC20Metadata(decimals);
        
        uint256 amount = 100000000000000001; // 1.00000000000000001 ERC20 tokens

        // Call the conversion function
        (uint64 convertedAmount, uint256 remainder) = ERC20CosmosCoinConversion._convertERC20AmountToCosmosCoin(address(customMockERC20Metadata), amount);

        // Expected values for 18 decimals
        uint64 expectedConvertedAmount = 1000000; // 1,000,000 Cosmos coins
        uint256 expectedRemainder = 1;

        // Assertions
        assertEq(convertedAmount, expectedConvertedAmount, "Converted amount mismatch");
        assertEq(remainder, expectedRemainder, "Remainder mismatch");
    }

    function testConvertMockERC20MetadataAmountToCosmosCoin_3() public  {
        uint8 decimals = 12; 
        // Deploy the ERC20 token with metadata (custom decimals)
        MockERC20Metadata customMockERC20Metadata = new MockERC20Metadata(decimals);
        
        uint256 amount = 1000000100001; // 1.000000000001 ERC20 tokens

        // Call the conversion function
        (uint64 convertedAmount, uint256 remainder) = ERC20CosmosCoinConversion._convertERC20AmountToCosmosCoin(address(customMockERC20Metadata), amount);

        // Expected values for 18 decimals
        uint64 expectedConvertedAmount = 1000000; // 1,000,000 Cosmos coins
        uint256 expectedRemainder = 100001;

        // Assertions
        assertEq(convertedAmount, expectedConvertedAmount, "Converted amount mismatch");
        assertEq(remainder, expectedRemainder, "Remainder mismatch");
    }

    function testConvertMockERC20MetadataAmountToCosmosCoin_4() public  {
        uint8 decimals = 7; 
        // Deploy the ERC20 token with metadata (custom decimals)
        MockERC20Metadata customMockERC20Metadata = new MockERC20Metadata(decimals);
        
        uint256 amount = 10000001; // 1.0000001 ERC20 tokens

        // Call the conversion function
        (uint64 convertedAmount, uint256 remainder) = ERC20CosmosCoinConversion._convertERC20AmountToCosmosCoin(address(customMockERC20Metadata), amount);

        // Expected values for 18 decimals
        uint64 expectedConvertedAmount = 1000000; // 1,000,000 Cosmos coins
        uint256 expectedRemainder = 1;

        // Assertions
        assertEq(convertedAmount, expectedConvertedAmount, "Converted amount mismatch");
        assertEq(remainder, expectedRemainder, "Remainder mismatch");
    }

    function testConvertMockERC20MetadataAmountToCosmosCoin_5() public  {
        uint8 decimals = 6; 
        // Deploy the ERC20 token with metadata (custom decimals)
        MockERC20Metadata customMockERC20Metadata = new MockERC20Metadata(decimals);
        
        uint256 amount = 1000001; // 1.000001 ERC20 tokens

        // Call the conversion function
        (uint64 convertedAmount, uint256 remainder) = ERC20CosmosCoinConversion._convertERC20AmountToCosmosCoin(address(customMockERC20Metadata), amount);

        // Expected values for 18 decimals
        uint64 expectedConvertedAmount = 1000001; // 1,000,001 Cosmos coins
        uint256 expectedRemainder = 0;

        // Assertions
        assertEq(convertedAmount, expectedConvertedAmount, "Converted amount mismatch");
        assertEq(remainder, expectedRemainder, "Remainder mismatch");
    }

/* Keeping this tests commented here, for now, in case we then decide to support < than 6 decimals 
    function testConvertMockERC20MetadataAmountToCosmosCoin_6() public  {
        uint8 decimals = 5; 
        // Deploy the ERC20 token with metadata (custom decimals)
        MockERC20Metadata customMockERC20Metadata = new MockERC20Metadata(decimals);
        
        uint256 amount = 100001; // 1.00001 ERC20 tokens

        // Call the conversion function
        (uint64 convertedAmount, uint256 remainder) = ERC20CosmosCoinConversion._convertERC20AmountToCosmosCoin(address(customMockERC20Metadata), amount);

        // Expected values for 18 decimals
        uint64 expectedConvertedAmount = 1000010; // 1,000,010 Cosmos coins
        uint256 expectedRemainder = 0;

        // Assertions
        assertEq(convertedAmount, expectedConvertedAmount, "Converted amount mismatch");
        assertEq(remainder, expectedRemainder, "Remainder mismatch");
    }

    function testConvertMockERC20MetadataAmountToCosmosCoin_7() public  {
        uint8 decimals = 3; 
        // Deploy the ERC20 token with metadata (custom decimals)
        MockERC20Metadata customMockERC20Metadata = new MockERC20Metadata(decimals);
        
        uint256 amount = 1001; // 1.001 ERC20 tokens

        // Call the conversion function
        (uint64 convertedAmount, uint256 remainder) = ERC20CosmosCoinConversion._convertERC20AmountToCosmosCoin(address(customMockERC20Metadata), amount);

        // Expected values for 18 decimals
        uint64 expectedConvertedAmount = 1001000; // 1,000,010 Cosmos coins
        uint256 expectedRemainder = 0;

        // Assertions
        assertEq(convertedAmount, expectedConvertedAmount, "Converted amount mismatch");
        assertEq(remainder, expectedRemainder, "Remainder mismatch");
    }

    function testConvertMockERC20MetadataAmountToCosmosCoin_8() public  {
        uint8 decimals = 1; 
        // Deploy the ERC20 token with metadata (custom decimals)
        MockERC20Metadata customMockERC20Metadata = new MockERC20Metadata(decimals);
        
        uint256 amount = 11; // 1.1 ERC20 tokens

        // Call the conversion function
        (uint64 convertedAmount, uint256 remainder) = ERC20CosmosCoinConversion._convertERC20AmountToCosmosCoin(address(customMockERC20Metadata), amount);

        // Expected values for 18 decimals
        uint64 expectedConvertedAmount = 1100000; // 1,000,010 Cosmos coins
        uint256 expectedRemainder = 0;

        // Assertions
        assertEq(convertedAmount, expectedConvertedAmount, "Converted amount mismatch");
        assertEq(remainder, expectedRemainder, "Remainder mismatch");
    }

    function testConvertMockERC20MetadataAmountToCosmosCoin_9() public  {
        uint8 decimals = 0; 
        // Deploy the ERC20 token with metadata (custom decimals)
        MockERC20Metadata customMockERC20Metadata = new MockERC20Metadata(decimals);
        
        uint256 amount = 1; // 1 ERC20 tokens

        // Call the conversion function
        (uint64 convertedAmount, uint256 remainder) = ERC20CosmosCoinConversion._convertERC20AmountToCosmosCoin(address(customMockERC20Metadata), amount);

        // Expected values for 18 decimals
        uint64 expectedConvertedAmount = 1000000; // 1,000,010 Cosmos coins
        uint256 expectedRemainder = 0;

        // Assertions
        assertEq(convertedAmount, expectedConvertedAmount, "Converted amount mismatch");
        assertEq(remainder, expectedRemainder, "Remainder mismatch");
    }
*/
    function testConvertMockERC20MetadataAmountToCosmosCoin_10() public  {
        uint8 decimals = 7; 
        // Deploy the ERC20 token with metadata (custom decimals)
        MockERC20Metadata customMockERC20Metadata_1 = new MockERC20Metadata(decimals);
        
        // Interesting: https://ethereum.stackexchange.com/questions/135557/covert-erc-20-tokens-with-different-decimals-to-amount-to-wei
        // Note that using this 10000001111 an input the decimals will be counted starting from last digit, the rest will be counted
        // as the entire part   
        uint256 amount = 10000001111; // 1000.0001111 ERC20 tokens

        // Call the conversion function
        (uint64 convertedAmount, uint256 remainder) = ERC20CosmosCoinConversion._convertERC20AmountToCosmosCoin(address(customMockERC20Metadata_1), amount);

        // Expected values for 18 decimals
        uint64 expectedConvertedAmount = 1000000111; // 1000,000,111 Cosmos coins
        uint256 expectedRemainder = 1;

        // Assertions
        assertEq(decimals, customMockERC20Metadata_1.decimals(), "Decimals mismatch");
        assertEq(convertedAmount, expectedConvertedAmount, "Converted amount mismatch");
        assertEq(remainder, expectedRemainder, "Remainder mismatch");
    }

/*
    // This should be always ok, convering smaller type uint64 into uint256
    function testConvertCosmosCoinAmountToERC20() pure public {
        uint256 convertedAmount = ERC20CosmosCoinConversion._convertCosmosCoinAmountToERC20(1000000); // 1,000,000 Cosmos coins
        uint256 expectedConvertedAmount = 1000000000000000000; // Should convert to 1 ERC20 token
        assertEq(convertedAmount, expectedConvertedAmount);
    }

/*
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
    */
}
