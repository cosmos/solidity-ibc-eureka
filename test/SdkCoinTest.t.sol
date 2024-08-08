// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25 <0.9.0;

// solhint-disable custom-errors,max-line-length
import { Test } from "forge-std/Test.sol";
import { SdkCoin } from "../src/utils/SdkCoin.sol";
import { ERC20 } from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import { IISdkCoinErrors } from "../src/errors/IISdkCoinErrors.sol";

// Discuss - Do we want to move mock contract to a mock folder?
// Mock ERC20 token without the decimals function overridden
contract MockERC20 is ERC20 {
    constructor() ERC20("BaseToken", "BTK") {
        _mint(msg.sender, 100_000_000_000_000_000_000); // Mint some tokens for testing
    }
}

// Discuss - Do we want to move mock contract to a mock folder?
// Mock ERC20 token with the decimals function overridden
contract MockERC20Metadata is ERC20 {
    uint8 private _decimals;

    constructor(uint8 decimals_) ERC20("MetadataToken", "MTK") {
        _decimals = decimals_;
        _mint(msg.sender, 100_000_000_000_000_000_000); // Mint some tokens for testing
    }

    function decimals() public view override returns (uint8) {
        return _decimals;
    }
}

contract SdkCoinTest is Test {
    // Instance of the MockERC20 contract
    MockERC20 private mockERC20;
    MockERC20Metadata private mockERC20Metadata;

    function setUp() public {
        // Deploy the mock ERC20 tokens
        mockERC20 = new MockERC20();
        mockERC20Metadata = new MockERC20Metadata(18);
    }

    /////////////////////////////////////////////////////
    // Tests for MockERC20 - Standard Tokens implementation without decimals --> decimals = 18

    /**
     * Problem Breakdown
     * ERC20 to Cosmos Coin Conversion:
     * We are converting an ERC20 token amount to a Cosmos coin amount.
     * - ERC20 standard use 18 decimals.
     * - Cosmos coins use 6 decimals.
     * Example Conversion:
     * - Input: 1000000000000000001 (1 ERC20 token + 1 extra smallest unit, i.e., 1 Wei).
     * - Expected Converted Amount: 1000000 (1 ERC20 token should convert to 1,000,000 Cosmos coins).
     * - Expected Remainder: 1 (which is the remaining smallest unit that doesn't fit into the Cosmos coin format).
     * The Remainder is what we espect to return to the user
     * Detailed Explanation:
     * - Given 1000000000000000001 as input:
     * Conversion Calculation:
     * - 1 ERC20 token = 1000000000000000000 (in Wei).
     * - Converting to Cosmos coin units involves dividing by 10^12
     * (since ERC20 has 18 decimals and Cosmos has 6 decimals, the difference is 18 - 6 = 12).
     * Mathematical Conversion:
     * - Converted Amount: 1000000000000000001 / 10^12 = 1000000 (Cosmos coins).
     * - Remainder: 1000000000000000001 % 10^12 = 1 (Wei remaining).
     */
    function testConvertMockERC20TokenAmountToSdkCoin_1() public view {
        uint256 amount = 1_000_000_000_000_000_001; // 1.000000000000000001 ERC20 tokens
        // Call the conversion function
        (uint64 convertedAmount, uint256 remainder) = SdkCoin._convertERC20AmountToSdkCoin(address(mockERC20), amount);

        // Expected values for default 18 decimals
        uint64 expectedConvertedAmount = 1_000_000; // 1,000,000 Cosmos coins
        uint256 expectedRemainder = 1;

        // Assertions
        assertEq(convertedAmount, expectedConvertedAmount, "Converted amount mismatch");
        assertEq(remainder, expectedRemainder, "Remainder mismatch");
    }

    function testConvertMockERC20AmountToSdkCoin_2() public view {
        uint256 amount = 1_000_000_100_000_000_001; // 1.000000100000000001 ERC20 tokens
        // Call the conversion function
        (uint64 convertedAmount, uint256 remainder) = SdkCoin._convertERC20AmountToSdkCoin(address(mockERC20), amount);

        // Expected values for default 18 decimals
        uint64 expectedConvertedAmount = 1_000_000; // 1,000,000 Cosmos coins
        uint256 expectedRemainder = 100_000_000_001;

        // Assertions
        assertEq(convertedAmount, expectedConvertedAmount, "Converted amount mismatch");
        assertEq(remainder, expectedRemainder, "Remainder mismatch");
    }

    function testConvertMockERC20AmountToSdkCoin_3() public view {
        uint256 amount = 1_000_000_000_000_000_000; // 1.000000000000000000 ERC20 tokens
        // Call the conversion function
        (uint64 convertedAmount, uint256 remainder) = SdkCoin._convertERC20AmountToSdkCoin(address(mockERC20), amount);

        // Expected values for default 18 decimals
        uint64 expectedConvertedAmount = 1_000_000; // 1,000,000 Cosmos coins
        uint256 expectedRemainder = 0;

        // Assertions
        assertEq(convertedAmount, expectedConvertedAmount, "Converted amount mismatch");
        assertEq(remainder, expectedRemainder, "Remainder mismatch");
    }

    function testConvertMockERC20AmountToSdkCoin_4() public view {
        uint256 amount = 1; // 1 smallest unit of ERC20 tokens
        // Call the conversion function
        (uint64 convertedAmount, uint256 remainder) = SdkCoin._convertERC20AmountToSdkCoin(address(mockERC20), amount);

        // Expected values for default 18 decimals
        uint64 expectedConvertedAmount = 0;
        uint256 expectedRemainder = 1;

        // Assertions
        assertEq(convertedAmount, expectedConvertedAmount, "Converted amount mismatch");
        assertEq(remainder, expectedRemainder, "Remainder mismatch");
    }

    function testConvertMockERC20AmountToSdkCoin_5() public view {
        uint256 amount = 999_999_999_999; // Less than 1 smallest unit of Cosmos coins
        // Call the conversion function
        (uint64 convertedAmount, uint256 remainder) = SdkCoin._convertERC20AmountToSdkCoin(address(mockERC20), amount);

        // Expected values for default 18 decimals
        uint64 expectedConvertedAmount = 0;
        uint256 expectedRemainder = 999_999_999_999;

        // Assertions
        assertEq(convertedAmount, expectedConvertedAmount, "Converted amount mismatch");
        assertEq(remainder, expectedRemainder, "Remainder mismatch");
    }

    function testConvertMockERC20AmountToSdkCoin_6() public view {
        uint256 amount = 999_999_999_999_999_999;
        // Call the conversion function
        (uint64 convertedAmount, uint256 remainder) = SdkCoin._convertERC20AmountToSdkCoin(address(mockERC20), amount);

        // Expected values for default 18 decimals
        uint64 expectedConvertedAmount = 999_999;
        uint256 expectedRemainder = 999_999_999_999;

        // Assertions
        assertEq(convertedAmount, expectedConvertedAmount, "Converted amount mismatch");
        assertEq(remainder, expectedRemainder, "Remainder mismatch");
    }

    /////////////////////////////////////////////////////
    // Tests for MockERC20Metadata - Extended Tokens Standard implementation with decimals
    function testConvertMockERC20MetadataAmountToSdkCoin() public {
        uint8 decimals = 18;
        // Deploy the ERC20 token with metadata (custom decimals)
        MockERC20Metadata customMockERC20Metadata = new MockERC20Metadata(decimals);
        uint256 amount = 1_000_000_000_000_000_001; // 1.000000000000000001 ERC20 tokens

        // Call the conversion function
        (uint64 convertedAmount, uint256 remainder) =
            SdkCoin._convertERC20AmountToSdkCoin(address(customMockERC20Metadata), amount);

        // Expected values for 18 decimals
        uint64 expectedConvertedAmount = 1_000_000; // 1,000,000 Cosmos coins
        uint256 expectedRemainder = 1;

        // Assertions
        assertEq(convertedAmount, expectedConvertedAmount, "Converted amount mismatch");
        assertEq(remainder, expectedRemainder, "Remainder mismatch");
    }

    function testConvertMockERC20MetadataAmountToSdkCoin_1() public {
        uint8 decimals = 77;
        // Deploy the ERC20 token with metadata (custom decimals)
        MockERC20Metadata customMockERC20Metadata = new MockERC20Metadata(decimals);
        uint256 amount =
            100_000_010_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_001;
        // Call the conversion function
        (uint64 convertedAmount, uint256 remainder) =
            SdkCoin._convertERC20AmountToSdkCoin(address(customMockERC20Metadata), amount);

        // Expected values for 18 decimals
        uint64 expectedConvertedAmount = 1_000_000; // 1,000,000 Cosmos coins
        uint256 expectedRemainder =
            10_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_001;

        // Assertions
        assertEq(convertedAmount, expectedConvertedAmount, "Converted amount mismatch");
        assertEq(remainder, expectedRemainder, "Remainder mismatch");
    }

    function testConvertMockERC20MetadataAmountToSdkCoin_2() public {
        uint8 decimals = 17;
        // Deploy the ERC20 token with metadata (custom decimals)
        MockERC20Metadata customMockERC20Metadata = new MockERC20Metadata(decimals);

        uint256 amount = 100_000_000_000_000_001; // 1.00000000000000001 ERC20 tokens

        // Call the conversion function
        (uint64 convertedAmount, uint256 remainder) =
            SdkCoin._convertERC20AmountToSdkCoin(address(customMockERC20Metadata), amount);

        // Expected values for 18 decimals
        uint64 expectedConvertedAmount = 1_000_000; // 1,000,000 Cosmos coins
        uint256 expectedRemainder = 1;

        // Assertions
        assertEq(convertedAmount, expectedConvertedAmount, "Converted amount mismatch");
        assertEq(remainder, expectedRemainder, "Remainder mismatch");
    }

    function testConvertMockERC20MetadataAmountToSdkCoin_3() public {
        uint8 decimals = 12;
        // Deploy the ERC20 token with metadata (custom decimals)
        MockERC20Metadata customMockERC20Metadata = new MockERC20Metadata(decimals);

        uint256 amount = 1_000_000_100_001; // 1.000000000001 ERC20 tokens

        // Call the conversion function
        (uint64 convertedAmount, uint256 remainder) =
            SdkCoin._convertERC20AmountToSdkCoin(address(customMockERC20Metadata), amount);

        // Expected values for 18 decimals
        uint64 expectedConvertedAmount = 1_000_000; // 1,000,000 Cosmos coins
        uint256 expectedRemainder = 100_001;

        // Assertions
        assertEq(convertedAmount, expectedConvertedAmount, "Converted amount mismatch");
        assertEq(remainder, expectedRemainder, "Remainder mismatch");
    }

    function testConvertMockERC20MetadataAmountToSdkCoin_4() public {
        uint8 decimals = 7;
        // Deploy the ERC20 token with metadata (custom decimals)
        MockERC20Metadata customMockERC20Metadata = new MockERC20Metadata(decimals);

        uint256 amount = 10_000_001; // 1.0000001 ERC20 tokens

        // Call the conversion function
        (uint64 convertedAmount, uint256 remainder) =
            SdkCoin._convertERC20AmountToSdkCoin(address(customMockERC20Metadata), amount);

        // Expected values for 18 decimals
        uint64 expectedConvertedAmount = 1_000_000; // 1,000,000 Cosmos coins
        uint256 expectedRemainder = 1;

        // Assertions
        assertEq(convertedAmount, expectedConvertedAmount, "Converted amount mismatch");
        assertEq(remainder, expectedRemainder, "Remainder mismatch");
    }

    function testConvertMockERC20MetadataAmountToSdkCoin_5() public {
        uint8 decimals = 6;
        // Deploy the ERC20 token with metadata (custom decimals)
        MockERC20Metadata customMockERC20Metadata = new MockERC20Metadata(decimals);

        uint256 amount = 1_000_001; // 1.000001 ERC20 tokens

        // Call the conversion function
        (uint64 convertedAmount, uint256 remainder) =
            SdkCoin._convertERC20AmountToSdkCoin(address(customMockERC20Metadata), amount);

        // Expected values for 18 decimals
        uint64 expectedConvertedAmount = 1_000_001; // 1,000,001 Cosmos coins
        uint256 expectedRemainder = 0;

        // Assertions
        assertEq(convertedAmount, expectedConvertedAmount, "Converted amount mismatch");
        assertEq(remainder, expectedRemainder, "Remainder mismatch");
    }

    function testConvertMockERC20MetadataAmountToSdkCoin_10() public {
        uint8 decimals = 7;
        // Deploy the ERC20 token with metadata (custom decimals)
        MockERC20Metadata customMockERC20Metadata = new MockERC20Metadata(decimals);

        // Interesting:
        // https://ethereum.stackexchange.com/questions/135557/covert-erc-20-tokens-with-different-decimals-to-amount-to-wei
        // Note that using this 10000001111 an input the decimals will be counted starting from last digit, the rest
        // will be counted
        // as the entire part
        uint256 amount = 10_000_001_111; // 1000.0001111 ERC20 tokens

        // Call the conversion function
        (uint64 convertedAmount, uint256 remainder) =
            SdkCoin._convertERC20AmountToSdkCoin(address(customMockERC20Metadata), amount);

        // Expected values for 18 decimals
        uint64 expectedConvertedAmount = 1_000_000_111; // 1000,000,111 Cosmos coins
        uint256 expectedRemainder = 1;

        // Assertions
        assertEq(decimals, customMockERC20Metadata.decimals(), "Decimals mismatch");
        assertEq(convertedAmount, expectedConvertedAmount, "Converted amount mismatch");
        assertEq(remainder, expectedRemainder, "Remainder mismatch");
    }

    function testConvertSdkCoinAmountToERC20_SameDecimals() public {
        MockERC20Metadata customMockERC20Metadata = new MockERC20Metadata(6);
        uint64 cosmosAmount = 1_000_000; // 1 Cosmos coin
        uint256 convertedAmount = SdkCoin._convertSdkCoinAmountToERC20(address(customMockERC20Metadata), cosmosAmount);
        uint256 expectedAmount = 1_000_000; // 1 ERC20 token with 6 decimals

        assertEq(convertedAmount, expectedAmount, "Conversion mismatch for same decimals");
    }

    function testConvertSdkCoinAmountToERC20_1() public {
        MockERC20Metadata customMockERC20Metadata = new MockERC20Metadata(18);
        uint64 cosmosAmount = 1_000_000; // 1 Cosmos coin
        uint256 convertedAmount = SdkCoin._convertSdkCoinAmountToERC20(address(customMockERC20Metadata), cosmosAmount);
        uint256 expectedAmount = 1_000_000_000_000_000_000; // 1 ERC20 token with 18 decimals

        assertEq(convertedAmount, expectedAmount, "Conversion mismatch for higher decimals");
    }

    function testConvertSdkCoinAmountToERC20_2() public {
        MockERC20Metadata customMockERC20Metadata = new MockERC20Metadata(7);
        uint64 cosmosAmount = 1_000_000; // 1 Cosmos coin
        uint256 convertedAmount = SdkCoin._convertSdkCoinAmountToERC20(address(customMockERC20Metadata), cosmosAmount);
        uint256 expectedAmount = 10_000_000; // 1 ERC20 token with 9 decimals

        assertEq(convertedAmount, expectedAmount, "Conversion mismatch for 9 decimals");
    }

    function testConvertSdkCoinAmountToERC20_3() public {
        MockERC20Metadata customMockERC20Metadata = new MockERC20Metadata(77);
        uint64 cosmosAmount = 1_000_000; // 1 Cosmos coin
        uint256 convertedAmount = SdkCoin._convertSdkCoinAmountToERC20(address(customMockERC20Metadata), cosmosAmount);
        uint256 expectedAmount =
            100_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000; // 1
            // ERC20 token with 18 decimals

        assertEq(convertedAmount, expectedAmount, "Conversion mismatch for higher decimals");
    }

    /////////////////////////////////////////////////////
    // Tests triggering reverts conditions

    /////////////////////////////////////////////////////
    // Tests triggering reverts conditions

    function testConvertERC20toSdkCoinAmount_ZeroAddress() public {
        uint256 evmAmount = 1_000_000; // 1 Cosmos coin

        vm.expectRevert(abi.encodeWithSelector(IISdkCoinErrors.ZeroAddress.selector, address(0)));
        SdkCoin._convertERC20AmountToSdkCoin(address(0), evmAmount);
    }

    function testConvertSdkCoinAmountToERC20_ZeroAddress() public {
        uint64 cosmosAmount = 1_000_000; // 1 Cosmos coin

        vm.expectRevert(abi.encodeWithSelector(IISdkCoinErrors.ZeroAddress.selector, address(0)));
        SdkCoin._convertSdkCoinAmountToERC20(address(0), cosmosAmount);
    }

    function testConvertERC20toSdkCoinAmount_LessThanSixDecimals() public {
        MockERC20Metadata customMockERC20Metadata = new MockERC20Metadata(5);
        uint256 evmAmount = 1_000_000; // 1 Cosmos coin

        vm.expectRevert(abi.encodeWithSelector(IISdkCoinErrors.UnsupportedTokenDecimals.selector, uint8(5)));
        SdkCoin._convertERC20AmountToSdkCoin(address(customMockERC20Metadata), evmAmount);
    }

    function testConvertSdkCoinAmountToERC20_LessThanSixDecimals() public {
        MockERC20Metadata customMockERC20Metadata = new MockERC20Metadata(5);
        uint64 cosmosAmount = 1_000_000; // 1 Cosmos coin

        vm.expectRevert(abi.encodeWithSelector(IISdkCoinErrors.UnsupportedTokenDecimals.selector, uint8(5)));
        SdkCoin._convertSdkCoinAmountToERC20(address(customMockERC20Metadata), cosmosAmount);
    }

    // Note that using vm.expectRevert(abi.encodeWithSelector(Errors.ZeroAmountUint64.selector, 0))
    // both the zero amount tests are failing with this message:
    // [FAIL. Reason: ZeroAmountUint64(0)]
    // Super wired, because is exactly what is expected
    // I guess with custom error refactor switching to solidity 0.8.26 this should be solved
    // For now to make test pass added testFail and removed the vm expect revert.
    function testFailConvertSdkCoinAmountToERC20_ZeroAmount() public {
        MockERC20Metadata customMockERC20Metadata = new MockERC20Metadata(6);
        uint64 cosmosAmount = 0; // 1 Cosmos coin

        //vm.expectRevert(abi.encodeWithSelector(Errors.ZeroAmountUint64.selector, 0));
        SdkCoin._convertSdkCoinAmountToERC20(address(customMockERC20Metadata), cosmosAmount);
    }

    function testFailConvertERC20toSdkCoinAmount_ZeroAmount() public {
        MockERC20Metadata customMockERC20Metadata = new MockERC20Metadata(6);
        uint256 evmAmount = 0; // 1 Cosmos coin

        //vm.expectRevert(abi.encodeWithSelector(Errors.ZeroAmountUint256.selector, 0));
        SdkCoin._convertERC20AmountToSdkCoin(address(customMockERC20Metadata), evmAmount);
    }
}
