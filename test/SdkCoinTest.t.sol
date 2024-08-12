// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25 <0.9.0;

// solhint-disable custom-errors,max-line-length
import { Test } from "forge-std/Test.sol";
import { SdkCoin } from "../src/utils/SdkCoin.sol";
import { IISdkCoinErrors } from "../src/errors/IISdkCoinErrors.sol";
import { SafeCast } from "@openzeppelin/contracts/utils/math/SafeCast.sol";
import { MockERC20 } from "../test/mock/MockERC20.sol";
import { MockERC20Metadata } from "../test/mock/MockERC20Metadata.sol";

/**
 * This test file validates the conversion functions between ERC20 token amounts and Cosmos SDK coin amounts,
 * considering the differences in decimals. The testing strategy is twofold:
 *
 * 1. Table Testing:
 *    - Involves creating a set of predefined test cases with specific inputs and expected outputs.
 *    - Each test case is manually computed to cover various scenarios, including normal operation, edge cases,
 *      and potential failure conditions.
 *
 * 2. Invariant Testing:
 *    - Combines fuzz testing with invariant testing.
 *    - Fuzz testing generates random inputs to uncover unexpected bugs and edge cases.
 *    - Invariant testing ensures that core properties and conditions always hold true for the function's outputs
 *      given random inputs.
 *    - This approach provides robust validation without the need to manually compute expected values for each input,
 *      enhancing the overall reliability and accuracy of the conversion functions.
 */
contract SdkCoinTest is Test, IISdkCoinErrors {
    // Instance of the MockERC20 contract
    MockERC20 private mockERC20;

    function setUp() public {
        // Deploy the mock ERC20 tokens
        mockERC20 = new MockERC20();
    }

    struct ERC20toSdkCoin_ConvertTestCase {
        string m;
        uint256 amount;
        uint64 expectedConvertedAmount;
        uint256 expectedRemainder;
    }

    /**
     * @notice Tests the conversion of ERC20 token amounts to SdkCoin amounts, ensuring that the conversion function
     *         accurately handles the conversion and calculates any remainder.
     *
     * @dev This function tests the conversion process from ERC20 tokens to SdkCoin, validating both the converted
     * amount
     *      and the remainder. The test cases cover various scenarios including exact conversions, scenarios with
     * remainders,
     *      and edge cases with very small or fractional amounts.
     *
     * The test cases cover the following scenarios:
     * - Conversion of 1.000000000000000001 ERC20 tokens
     * - Conversion of 1.000000100000000001 ERC20 tokens
     * - Conversion of 1.000000000000000000 ERC20 tokens (exact match)
     * - Conversion of 1 smallest unit of ERC20 tokens
     * - Conversion of less than 1 smallest unit of SdkCoin
     * - Conversion of 999.999999999999999999 ERC20 tokens
     *
     * The function asserts that:
     * - The converted amount matches the expected converted amount based on the ERC20 token amount.
     * - The remainder matches the expected remainder after the conversion.
     *
     */
    function test_ERC20toSdkCoin_ConvertAmount() public view {
        ERC20toSdkCoin_ConvertTestCase[] memory testCases = new ERC20toSdkCoin_ConvertTestCase[](6);

        testCases[0] = ERC20toSdkCoin_ConvertTestCase({
            m: "1.000000000000000001 ERC20 tokens",
            amount: 1_000_000_000_000_000_001,
            expectedConvertedAmount: 1_000_000,
            expectedRemainder: 1
        });

        testCases[1] = ERC20toSdkCoin_ConvertTestCase({
            m: "1.000000100000000001 ERC20 tokens",
            amount: 1_000_000_100_000_000_001,
            expectedConvertedAmount: 1_000_000,
            expectedRemainder: 100_000_000_001
        });

        testCases[2] = ERC20toSdkCoin_ConvertTestCase({
            m: "1.000000000000000000 ERC20 tokens",
            amount: 1_000_000_000_000_000_000,
            expectedConvertedAmount: 1_000_000,
            expectedRemainder: 0
        });

        testCases[3] = ERC20toSdkCoin_ConvertTestCase({
            m: "1 smallest unit of ERC20 tokens",
            amount: 1,
            expectedConvertedAmount: 0,
            expectedRemainder: 1
        });

        testCases[4] = ERC20toSdkCoin_ConvertTestCase({
            m: "Less than 1 smallest unit of SdkCoin",
            amount: 999_999_999_999,
            expectedConvertedAmount: 0,
            expectedRemainder: 999_999_999_999
        });

        testCases[5] = ERC20toSdkCoin_ConvertTestCase({
            m: "999.999999999999999999 ERC20 tokens",
            amount: 999_999_999_999_999_999,
            expectedConvertedAmount: 999_999,
            expectedRemainder: 999_999_999_999
        });

        for (uint256 i = 0; i < testCases.length; i++) {
            ERC20toSdkCoin_ConvertTestCase memory tc = testCases[i];
            (uint64 convertedAmount, uint256 remainder) =
                SdkCoin._ERC20ToSdkCoin_ConvertAmount(address(mockERC20), tc.amount);

            // Assertions
            assertEq(
                convertedAmount,
                tc.expectedConvertedAmount,
                string(abi.encodePacked("Converted amount mismatch: ", tc.m))
            );
            assertEq(remainder, tc.expectedRemainder, string(abi.encodePacked("Remainder mismatch: ", tc.m)));
        }
    }

    /////////////////////////////////////////////////////
    // Tests for MockERC20Metadata - Extended Tokens Standard implementation with decimals
    struct ERC20MetadataToSdkCoin_ConvertTestCase {
        string m;
        uint8 decimals;
        uint256 amount;
        uint64 expectedConvertedAmount;
        uint256 expectedRemainder;
    }

    function test_ERC20MetadataToSdkCoin_ConvertAmount() public {
        ERC20MetadataToSdkCoin_ConvertTestCase[] memory testCases = new ERC20MetadataToSdkCoin_ConvertTestCase[](7);

        testCases[0] = ERC20MetadataToSdkCoin_ConvertTestCase({
            m: "1.000000000000000001 ERC20 tokens with 18 decimals",
            decimals: 18,
            amount: 1_000_000_000_000_000_001,
            expectedConvertedAmount: 1_000_000,
            expectedRemainder: 1
        });

        testCases[1] = ERC20MetadataToSdkCoin_ConvertTestCase({
            m: "1.000000000000000001 ERC20 tokens with 77 decimals",
            decimals: 77,
            amount: 100_000_010_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_001,
            expectedConvertedAmount: 1_000_000,
            expectedRemainder: 10_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_001
        });

        testCases[2] = ERC20MetadataToSdkCoin_ConvertTestCase({
            m: "1.00000000000000001 ERC20 tokens with 17 decimals",
            decimals: 17,
            amount: 100_000_000_000_000_001,
            expectedConvertedAmount: 1_000_000,
            expectedRemainder: 1
        });

        testCases[3] = ERC20MetadataToSdkCoin_ConvertTestCase({
            m: "1.000000000001 ERC20 tokens with 12 decimals",
            decimals: 12,
            amount: 1_000_000_100_001,
            expectedConvertedAmount: 1_000_000,
            expectedRemainder: 100_001
        });

        testCases[4] = ERC20MetadataToSdkCoin_ConvertTestCase({
            m: "1.0000001 ERC20 tokens with 7 decimals",
            decimals: 7,
            amount: 10_000_001,
            expectedConvertedAmount: 1_000_000,
            expectedRemainder: 1
        });

        testCases[5] = ERC20MetadataToSdkCoin_ConvertTestCase({
            m: "1.000001 ERC20 tokens with 6 decimals",
            decimals: 6,
            amount: 1_000_001,
            expectedConvertedAmount: 1_000_001,
            expectedRemainder: 0
        });

        testCases[6] = ERC20MetadataToSdkCoin_ConvertTestCase({
            m: "1000.0001111 ERC20 tokens with 7 decimals",
            decimals: 7,
            amount: 10_000_001_111,
            expectedConvertedAmount: 1_000_000_111,
            expectedRemainder: 1
        });

        for (uint256 i = 0; i < testCases.length; i++) {
            ERC20MetadataToSdkCoin_ConvertTestCase memory tc = testCases[i];
            MockERC20Metadata mockERC20Metadata = new MockERC20Metadata(tc.decimals);
            (uint64 convertedAmount, uint256 remainder) =
                SdkCoin._ERC20ToSdkCoin_ConvertAmount(address(mockERC20Metadata), tc.amount);

            // Assertions
            assertEq(mockERC20Metadata.decimals(), tc.decimals, string(abi.encodePacked("Decimals mismatch: ", tc.m)));
            assertEq(
                convertedAmount,
                tc.expectedConvertedAmount,
                string(abi.encodePacked("Converted amount mismatch: ", tc.m))
            );
            assertEq(remainder, tc.expectedRemainder, string(abi.encodePacked("Remainder mismatch: ", tc.m)));
        }
    }

    /////////////////////////////////////////////////////
    // Tests for SdkCoin to ERC20Metadata - Extended Tokens Standard implementation with decimals

    struct SdkCoinToERC20_ConvertTestCase {
        string m;
        uint8 decimals;
        uint64 cosmosAmount;
        uint256 expectedConvertedAmount;
    }

    /**
     * @notice Tests the conversion of SdkCoin amounts to ERC20 token amounts based on different decimal places.
     *
     * @dev This function tests the conversion process of SdkCoin to ERC20 tokens with various decimals, ensuring that
     * the
     *      conversion function `_SdkCoinToERC20_ConvertAmount` correctly calculates the equivalent ERC20 token amount.
     *
     * The test cases cover the following scenarios:
     * - 1 SdkCoin to an ERC20 token with 6 decimals
     * - 1 SdkCoin to an ERC20 token with 18 decimals
     * - 1 SdkCoin to an ERC20 token with 7 decimals
     * - 1 SdkCoin to an ERC20 token with 77 decimals
     *
     * The function asserts that:
     * - The decimals in the mock ERC20 metadata match the expected decimals.
     * - The converted amount matches the expected converted amount based on the SdkCoin amount and ERC20 decimals.
     */
    function test_SdkCoinToERC20Metadata_ConvertAmount() public {
        SdkCoinToERC20_ConvertTestCase[] memory testCases = new SdkCoinToERC20_ConvertTestCase[](4);

        testCases[0] = SdkCoinToERC20_ConvertTestCase({
            m: "1 SdkCoin to ERC20 token with 6 decimals",
            decimals: 6,
            cosmosAmount: 1_000_000,
            expectedConvertedAmount: 1_000_000
        });

        testCases[1] = SdkCoinToERC20_ConvertTestCase({
            m: "1 SdkCoin to ERC20 token with 18 decimals",
            decimals: 18,
            cosmosAmount: 1_000_000,
            expectedConvertedAmount: 1_000_000_000_000_000_000
        });

        testCases[2] = SdkCoinToERC20_ConvertTestCase({
            m: "1 SdkCoin to ERC20 token with 7 decimals",
            decimals: 7,
            cosmosAmount: 1_000_000,
            expectedConvertedAmount: 10_000_000
        });

        testCases[3] = SdkCoinToERC20_ConvertTestCase({
            m: "1 SdkCoin to ERC20 token with 77 decimals",
            decimals: 77,
            cosmosAmount: 1_000_000,
            expectedConvertedAmount: 100_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000
        });

        for (uint256 i = 0; i < testCases.length; i++) {
            SdkCoinToERC20_ConvertTestCase memory tc = testCases[i];
            MockERC20Metadata mockERC20Metadata = new MockERC20Metadata(tc.decimals);
            uint256 convertedAmount = SdkCoin._SdkCoinToERC20_ConvertAmount(address(mockERC20Metadata), tc.cosmosAmount);

            // Assertions
            assertEq(mockERC20Metadata.decimals(), tc.decimals, string(abi.encodePacked("Decimals mismatch: ", tc.m)));
            assertEq(
                convertedAmount,
                tc.expectedConvertedAmount,
                string(abi.encodePacked("Converted amount mismatch: ", tc.m))
            );
        }
    }

    /////////////////////////////////////////////////////
    // Tests triggering reverts conditions

    struct RevertTestCase {
        string m;
        address tokenAddress;
        uint8 decimals;
        uint256 evmAmount;
        uint64 cosmosAmount;
        bytes expectedRevertSelector;
    }

    /**
     * @notice Tests various revert conditions for ERC20 to SdkCoin and SdkCoin to ERC20 conversion functions.
     *
     * @dev This function ensures that the appropriate revert conditions are triggered under specific invalid scenarios.
     *      It covers cases such as using the zero address, unsupported token decimals, and zero conversion amounts.
     *
     * The test cases cover the following scenarios:
     * - Zero address provided for ERC20 to SdkCoin conversion
     * - Zero address provided for SdkCoin to ERC20 conversion
     * - Less than six decimals for ERC20 to SdkCoin conversion
     * - Less than six decimals for SdkCoin to ERC20 conversion
     * - Zero amount provided for SdkCoin to ERC20 conversion
     * - Zero amount provided for ERC20 to SdkCoin conversion
     *
     * The function asserts that:
     * - The specified revert condition is correctly triggered by the respective function calls.
     * - Each test case expects a specific revert selector to be emitted, ensuring that the contract behaves as expected
     *   when encountering invalid inputs.
     *
     * The test uses `vm.expectRevert` to check for expected reverts in each scenario. Depending on the input
     * parameters,
     * it calls either the `_ERC20ToSdkCoin_ConvertAmount` or `_SdkCoinToERC20_ConvertAmount` function and verifies that
     * the correct error is thrown.
     */
    function testRevertConditions() public {
        RevertTestCase[] memory testCases = new RevertTestCase[](6);

        testCases[0] = RevertTestCase({
            m: "Zero address for ERC20 to SdkCoin conversion",
            tokenAddress: address(0),
            decimals: 0,
            evmAmount: 1_000_000,
            cosmosAmount: 0,
            expectedRevertSelector: abi.encodeWithSelector(InvalidAddress.selector, address(0))
        });

        testCases[1] = RevertTestCase({
            m: "Zero address for SdkCoin to ERC20 conversion",
            tokenAddress: address(0),
            decimals: 0,
            evmAmount: 0,
            cosmosAmount: 1_000_000,
            expectedRevertSelector: abi.encodeWithSelector(InvalidAddress.selector, address(0))
        });

        testCases[2] = RevertTestCase({
            m: "Less than six decimals for ERC20 to SdkCoin conversion",
            tokenAddress: address(new MockERC20Metadata(5)),
            decimals: 5,
            evmAmount: 1_000_000,
            cosmosAmount: 0,
            expectedRevertSelector: abi.encodeWithSelector(UnsupportedTokenDecimals.selector, uint8(5))
        });

        testCases[3] = RevertTestCase({
            m: "Less than six decimals for SdkCoin to ERC20 conversion",
            tokenAddress: address(new MockERC20Metadata(5)),
            decimals: 5,
            evmAmount: 0,
            cosmosAmount: 1_000_000,
            expectedRevertSelector: abi.encodeWithSelector(UnsupportedTokenDecimals.selector, uint8(5))
        });

        testCases[4] = RevertTestCase({
            m: "Zero amount for SdkCoin to ERC20 conversion",
            tokenAddress: address(new MockERC20Metadata(6)),
            decimals: 6,
            evmAmount: 0,
            cosmosAmount: 0,
            expectedRevertSelector: abi.encodeWithSelector(InvalidAmount.selector, uint256(0))
        });
        testCases[5] = RevertTestCase({
            m: "Zero amount for ERC20 to SdkCoin conversion",
            tokenAddress: address(new MockERC20Metadata(6)),
            decimals: 6,
            evmAmount: 0,
            cosmosAmount: 0,
            expectedRevertSelector: abi.encodeWithSelector(InvalidAmount.selector, uint256(0))
        });

        for (uint256 i = 0; i < testCases.length; i++) {
            RevertTestCase memory tc = testCases[i];

            vm.expectRevert(tc.expectedRevertSelector);
            if (tc.evmAmount != 0) {
                SdkCoin._ERC20ToSdkCoin_ConvertAmount(tc.tokenAddress, tc.evmAmount);
            } else {
                SdkCoin._SdkCoinToERC20_ConvertAmount(tc.tokenAddress, tc.cosmosAmount);
            }
        }
    }

    ///////////////////////////////////////
    // We now provide the invariant testing for the conversion functions, which includes fuzz testing.

    ///////////////////////////////////////
    // Invariants:

    /**
     * @notice Assert that the remainder is zero
     * @notice This function ensures that the remainder is zero for certain conditions
     * @param remainder The remainder value to be checked
     */
    function invariant_ERC20toSdkCoin_RemainderIsZero(uint256 remainder) internal pure {
        // Assert that the remainder is zero with a custom error message
        if (remainder != 0) {
            revert RemainderIsNotZero(remainder);
        }
    }

    /**
     * @notice Validate the remainder based on the given factor
     * @notice This function checks if the remainder is correctly calculated based on whether the amount is an exact
     * multiple of the factor
     * @param amount The amount to be converted
     * @param remainder The remainder of the conversion
     * @param factor The factor used in the conversion
     */
    function invariant_ERC20toSdkCoin_RemainderByFactor(
        uint256 amount,
        uint256 remainder,
        uint256 factor
    )
        internal
        pure
    {
        // Check if the amount is not an exact multiple of the factor
        if (amount % factor != 0 && remainder == 0) {
            // If the amount is not an exact multiple of the factor, the remainder should not be zero
            // Revert with an error if the remainder is zero in this case
            revert RemainderIsNotBiggerThanZero(remainder);
        } else if (amount % factor == 0 && remainder != 0) {
            // Check if the amount is an exact multiple of the factor
            // If the amount is an exact multiple of the factor, the remainder should be zero
            // Revert with an error if the remainder is not zero in this case
            revert RemainderIsNotZero(remainder);
        }
    }

    /**
     * @notice Invariant check to ensure the converted amount equals the input amount
     * @notice Given random inputs, this check ensures that the converted amount equals the input amount for ERC20 to
     * Cosmos coin conversion
     * @param convertedAmount The converted amount to be checked
     * @param amount The original amount of ERC20 tokens
     */
    function invariant_ERC20toSdkCoin_ConvertedAmountEqualsInput(
        uint64 convertedAmount,
        uint256 amount
    )
        internal
        pure
    {
        if (convertedAmount != SafeCast.toUint64(amount)) {
            revert ConvertedAmountNotEqualInput(convertedAmount, amount);
        }
    }

    /**
     * @notice Invariant check to ensure the converted amount is greater than or equal to the input amount
     * @notice Given random inputs, this check ensures that the converted amount is greater than or equal to the input
     * amount for Cosmos coin to ERC20 conversion
     * @param convertedAmount The converted amount to be checked
     * @param amount The original amount of Cosmos coins
     */
    function invariant_SdkCointoERC20_ConvertedAmountBiggerOrEqualThanInput(
        uint256 convertedAmount,
        uint64 amount
    )
        internal
        pure
    {
        if (convertedAmount < amount) {
            revert ConvertedAmountNotBiggerThanOrEqualInput(convertedAmount, amount);
        }
    }

    /////////////////////////////////////
    // Let's define the assertions functions that use our invariants
    /**
     * @notice Assert invariants for ERC20 to Cosmos coin conversion when token decimals are equal to Cosmos decimals
     * @notice Given random inputs, this function applies invariant checking to ensure the converted amount equals the
     * input amount and the remainder is zero
     * @param convertedAmount The converted amount to be checked
     * @param amount The original amount of ERC20 tokens
     * @param remainder The remainder of the conversion
     */
    function assertInvariants_ERC20toSdkCoin_EqualDecimals(
        uint64 convertedAmount,
        uint256 amount,
        uint256 remainder
    )
        internal
        pure
    {
        invariant_ERC20toSdkCoin_ConvertedAmountEqualsInput(convertedAmount, amount);
        invariant_ERC20toSdkCoin_RemainderIsZero(remainder);
    }

    /**
     * @notice Assert invariants for ERC20 to Cosmos coin conversion when ERC20 token decimals are greater than Cosmos
     * decimals
     * @notice This function ensures that the remainder and converted amount are correctly calculated when the token
     * decimals are larger than the Cosmos decimals
     * @param convertedAmount The amount converted to uint64 supported by Cosmos coins
     * @param amount The original amount of ERC20 tokens
     * @param remainder The remainder of the conversion
     * @param factor The factor used for conversion based on the difference in decimals
     */
    function assertInvariants_ERC20toSdkCoin_BiggerDecimals(
        uint64 convertedAmount,
        uint256 amount,
        uint256 remainder,
        uint256 factor
    )
        internal
        pure
    {
        if (convertedAmount == amount) {
            invariant_ERC20toSdkCoin_RemainderIsZero(remainder);
        } else if (convertedAmount < SafeCast.toUint64(amount)) {
            invariant_ERC20toSdkCoin_RemainderByFactor(amount, remainder, factor);
        }
    }

    //////////////////////////////////////
    // Invariants Tests
    // Invariant test: ERC20 -> SdkCoin with tokenDecimals == DEFAULT_COSMOS_DECIMALS,
    /**
     * @notice Invariant test for ERC20 to Cosmos coin conversion when token decimals are equal to Cosmos decimals
     * @notice Given random inputs, this test applies invariant checking to ensure the converted amount equals the input
     * amount and the remainder is zero
     * @param amount The amount of ERC20 tokens to be converted
     * Requirements:
     * - `amount` must be greater than 0 and less than or equal to the maximum uint64 value.
     */
    function testInvariant_ERC20toSdkCoin_EqualDecimals(uint256 amount) public {
        // Skip test for zero amount or overflow conditions
        if (amount == 0 || amount > ~uint64(0)) {
            // These conditions will revert and are expected behaviours that have already been covered in table testing
            return;
        }
        address tokenAddress = address(new MockERC20Metadata(6));
        (uint64 convertedAmount, uint256 remainder) =
            SdkCoin._ERC20ToSdkCoin_ConvertAmount(address(tokenAddress), amount);
        assertInvariants_ERC20toSdkCoin_EqualDecimals(convertedAmount, amount, remainder);
    }

    // Invariant test: ERC20 -> SdkCoin with tokenDecimals > DEFAULT_COSMOS_DECIMALS
    /**
     * @notice Invariant test for ERC20 to Cosmos coin conversion with token decimals greater than Cosmos decimals
     * @notice Given random inputs, this test applies invariant checking previously defined
     * @param amount The amount of ERC20 tokens to be converted
     * @param decimals The number of decimals of the ERC20 token
     * Requirements:
     * - `amount` must be greater than 0 and less than or equal to the maximum uint64 value.
     * - `decimals` must be greater than the default Cosmos decimals (6) and less than or equal to 77.
     */
    function testInvariant_ERC20ToSdkCoin_BiggerDecimals(uint256 amount, uint8 decimals) public {
        // Inputs constraints
        if (amount == 0 || amount > ~uint64(0) || decimals <= 6 || decimals > 77) {
            // These conditions will revert and are expected behaviours that have already been covered in table testing
            return;
        }
        address tokenAddress = address(new MockERC20Metadata(decimals));
        uint256 factor = 10 ** (decimals - SdkCoin.DEFAULT_COSMOS_DECIMALS);

        (uint64 convertedAmount, uint256 remainder) =
            SdkCoin._ERC20ToSdkCoin_ConvertAmount(address(tokenAddress), amount);
        assertInvariants_ERC20toSdkCoin_BiggerDecimals(convertedAmount, amount, remainder, factor);
    }

    // Invariant test: SdkCoin -> ERC20
    /**
     * @notice Tests the conversion of Cosmos SDK coin amounts to ERC20 token amounts with varying decimals.
     * @dev This function validates the invariant that the converted ERC20 token amount is always greater than or equal
     * to the input Cosmos SDK coin amount.
     *
     * @param amount The amount of Cosmos SDK coins to be converted.
     * @param decimals The number of decimals for the ERC20 token.
     *
     * Requirements:
     * - `amount` must be greater than 0 and less than or equal to the maximum uint64 value.
     * - `decimals` must be between 6 and 77 (inclusive).
     * - The multiplication of `amount` and the conversion factor must not overflow.
     *
     * The function will skip test cases that do not meet these requirements.
     */
    function testInvariant_SdkCoinToERC20(uint64 amount, uint8 decimals) public {
        // Inputs constraints
        if (amount == 0 || decimals < 6 || decimals > 77) {
            // These conditions will revert and are expected behaviours that have already been covered in table testing
            return;
        }

        uint256 factor = 10 ** (decimals - SdkCoin.DEFAULT_COSMOS_DECIMALS);
        // Ensure the multiplication will not overflow given the random inputs
        if (amount > ~uint256(0) / factor) {
            // Skip the test case as it would cause an overflow that will be catched by built-in checks
            return;
        }
        address tokenAddress = address(new MockERC20Metadata(decimals));

        uint256 convertedAmount = SdkCoin._SdkCoinToERC20_ConvertAmount(address(tokenAddress), amount);
        invariant_SdkCointoERC20_ConvertedAmountBiggerOrEqualThanInput(convertedAmount, amount);
    }
}
