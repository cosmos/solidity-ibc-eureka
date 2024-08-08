// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25 <0.9.0;

// solhint-disable custom-errors,max-line-length
import { Test } from "forge-std/Test.sol";
import { SdkCoin } from "../src/utils/SdkCoin.sol";
import { ERC20 } from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import { IISdkCoinErrors } from "../src/errors/IISdkCoinErrors.sol";
import { SafeCast } from "@openzeppelin/contracts/utils/math/SafeCast.sol";

/*
    This test file validates the conversion functions between ERC20 token amounts and Cosmos SDK coin amounts,
    considering the differences in decimals. The testing strategy is twofold:

    1. **Table Testing**: This involves manually computing expected scenarios and failure conditions to ensure comprehensive
       coverage. It verifies that the conversion functions behave correctly across a wide range of predefined cases,
       including edge cases and common scenarios.

    2. **Invariant Testing**: This combines fuzz testing with invariant testing. Fuzz testing generates random inputs to 
       uncover unexpected bugs and edge cases, while invariant testing ensures that core properties and conditions 
       always hold true for the function's outputs. This approach provides robust validation without the need to 
       manually compute expected values for each input, enhancing the overall reliability and accuracy of the conversion functions.
*/

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

/*
    Table testing involves creating a set of predefined test cases with specific inputs and expected outputs. Each 
    test case is manually computed to cover various scenarios, including normal operation, edge cases, and potential 
    failure conditions. By explicitly defining the expected outcomes for these specific inputs, table testing allows us 
    to ensure that the conversion functions handle all possible scenarios correctly and robustly.

    The benefits of table testing include:
    - **Predictability**: By knowing the expected output for a given input, we can easily verify the correctness of 
      the function's behavior.
    - **Comprehensive Coverage**: We can systematically cover a wide range of scenarios, including edge cases that 
      might be missed by random input generation.
    - **Failure Scenarios**: Table testing allows us to explicitly define and test for failure conditions, ensuring 
      that the functions handle errors gracefully and as expected.
    - **Documentation**: Table tests serve as documentation of the function's expected behavior across different 
      scenarios, providing clarity and insight into the function's operation.

    In summary, table testing provides a solid foundation for verifying the correctness of our conversion functions, 
    ensuring that they perform accurately and reliably in a controlled set of predefined scenarios before moving on to 
    more dynamic testing approaches.
*/

contract SdkCoinTest is Test, IISdkCoinErrors {
    // Instance of the MockERC20 contract
    MockERC20 private mockERC20;

    function setUp() public {
        // Deploy the mock ERC20 tokens
        mockERC20 = new MockERC20();
    }

    struct ConvertSdkCoinTestCase {
        string m;
        uint256 amount;
        uint64 expectedConvertedAmount;
        uint256 expectedRemainder;
    }

    function testConvertMockERC20AmountToSdkCoin() public view {
        ConvertSdkCoinTestCase[] memory testCases = new ConvertSdkCoinTestCase[](6);

        testCases[0] = ConvertSdkCoinTestCase({
            m: "1.000000000000000001 ERC20 tokens",
            amount: 1_000_000_000_000_000_001,
            expectedConvertedAmount: 1_000_000,
            expectedRemainder: 1
        });

        testCases[1] = ConvertSdkCoinTestCase({
            m: "1.000000100000000001 ERC20 tokens",
            amount: 1_000_000_100_000_000_001,
            expectedConvertedAmount: 1_000_000,
            expectedRemainder: 100_000_000_001
        });

        testCases[2] = ConvertSdkCoinTestCase({
            m: "1.000000000000000000 ERC20 tokens",
            amount: 1_000_000_000_000_000_000,
            expectedConvertedAmount: 1_000_000,
            expectedRemainder: 0
        });

        testCases[3] = ConvertSdkCoinTestCase({
            m: "1 smallest unit of ERC20 tokens",
            amount: 1,
            expectedConvertedAmount: 0,
            expectedRemainder: 1
        });

        testCases[4] = ConvertSdkCoinTestCase({
            m: "Less than 1 smallest unit of Cosmos coins",
            amount: 999_999_999_999,
            expectedConvertedAmount: 0,
            expectedRemainder: 999_999_999_999
        });

        testCases[5] = ConvertSdkCoinTestCase({
            m: "999.999999999999999999 ERC20 tokens",
            amount: 999_999_999_999_999_999,
            expectedConvertedAmount: 999_999,
            expectedRemainder: 999_999_999_999
        });

        for (uint256 i = 0; i < testCases.length; i++) {
            ConvertSdkCoinTestCase memory tc = testCases[i];
            (uint64 convertedAmount, uint256 remainder) =
                SdkCoin._convertERC20AmountToSdkCoin(address(mockERC20), tc.amount);

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
    struct ConvertSdkCoinMetadataTestCase {
        string m;
        uint8 decimals;
        uint256 amount;
        uint64 expectedConvertedAmount;
        uint256 expectedRemainder;
    }

    function testConvertMockERC20MetadataAmountToSdkCoin() public {
        ConvertSdkCoinMetadataTestCase[] memory testCases = new ConvertSdkCoinMetadataTestCase[](7);

        testCases[0] = ConvertSdkCoinMetadataTestCase({
            m: "1.000000000000000001 ERC20 tokens with 18 decimals",
            decimals: 18,
            amount: 1_000_000_000_000_000_001,
            expectedConvertedAmount: 1_000_000,
            expectedRemainder: 1
        });

        testCases[1] = ConvertSdkCoinMetadataTestCase({
            m: "1.000000000000000001 ERC20 tokens with 77 decimals",
            decimals: 77,
            amount: 100_000_010_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_001,
            expectedConvertedAmount: 1_000_000,
            expectedRemainder: 10_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_001
        });

        testCases[2] = ConvertSdkCoinMetadataTestCase({
            m: "1.00000000000000001 ERC20 tokens with 17 decimals",
            decimals: 17,
            amount: 100_000_000_000_000_001,
            expectedConvertedAmount: 1_000_000,
            expectedRemainder: 1
        });

        testCases[3] = ConvertSdkCoinMetadataTestCase({
            m: "1.000000000001 ERC20 tokens with 12 decimals",
            decimals: 12,
            amount: 1_000_000_100_001,
            expectedConvertedAmount: 1_000_000,
            expectedRemainder: 100_001
        });

        testCases[4] = ConvertSdkCoinMetadataTestCase({
            m: "1.0000001 ERC20 tokens with 7 decimals",
            decimals: 7,
            amount: 10_000_001,
            expectedConvertedAmount: 1_000_000,
            expectedRemainder: 1
        });

        testCases[5] = ConvertSdkCoinMetadataTestCase({
            m: "1.000001 ERC20 tokens with 6 decimals",
            decimals: 6,
            amount: 1_000_001,
            expectedConvertedAmount: 1_000_001,
            expectedRemainder: 0
        });

        testCases[6] = ConvertSdkCoinMetadataTestCase({
            m: "1000.0001111 ERC20 tokens with 7 decimals",
            decimals: 7,
            amount: 10_000_001_111,
            expectedConvertedAmount: 1_000_000_111,
            expectedRemainder: 1
        });

        for (uint256 i = 0; i < testCases.length; i++) {
            ConvertSdkCoinMetadataTestCase memory tc = testCases[i];
            MockERC20Metadata customMockERC20Metadata = new MockERC20Metadata(tc.decimals);
            (uint64 convertedAmount, uint256 remainder) =
                SdkCoin._convertERC20AmountToSdkCoin(address(customMockERC20Metadata), tc.amount);

            // Assertions
            assertEq(
                customMockERC20Metadata.decimals(), tc.decimals, string(abi.encodePacked("Decimals mismatch: ", tc.m))
            );
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

    struct ConvertSdkCoinToERC20TestCase {
        string m;
        uint8 decimals;
        uint64 cosmosAmount;
        uint256 expectedConvertedAmount;
    }

    function testConvertSdkCoinAmountToERC20() public {
        ConvertSdkCoinToERC20TestCase[] memory testCases = new ConvertSdkCoinToERC20TestCase[](4);

        testCases[0] = ConvertSdkCoinToERC20TestCase({
            m: "1 Cosmos coin to ERC20 token with 6 decimals",
            decimals: 6,
            cosmosAmount: 1_000_000,
            expectedConvertedAmount: 1_000_000
        });

        testCases[1] = ConvertSdkCoinToERC20TestCase({
            m: "1 Cosmos coin to ERC20 token with 18 decimals",
            decimals: 18,
            cosmosAmount: 1_000_000,
            expectedConvertedAmount: 1_000_000_000_000_000_000
        });

        testCases[2] = ConvertSdkCoinToERC20TestCase({
            m: "1 Cosmos coin to ERC20 token with 7 decimals",
            decimals: 7,
            cosmosAmount: 1_000_000,
            expectedConvertedAmount: 10_000_000
        });

        testCases[3] = ConvertSdkCoinToERC20TestCase({
            m: "1 Cosmos coin to ERC20 token with 77 decimals",
            decimals: 77,
            cosmosAmount: 1_000_000,
            expectedConvertedAmount: 100_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000_000
        });

        for (uint256 i = 0; i < testCases.length; i++) {
            ConvertSdkCoinToERC20TestCase memory tc = testCases[i];
            MockERC20Metadata customMockERC20Metadata = new MockERC20Metadata(tc.decimals);
            uint256 convertedAmount =
                SdkCoin._convertSdkCoinAmountToERC20(address(customMockERC20Metadata), tc.cosmosAmount);

            // Assertions
            assertEq(
                customMockERC20Metadata.decimals(), tc.decimals, string(abi.encodePacked("Decimals mismatch: ", tc.m))
            );
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

    function testRevertConditions() public {
        RevertTestCase[] memory testCases = new RevertTestCase[](6);

        testCases[0] = RevertTestCase({
            m: "Zero address for ERC20 to SdkCoin conversion",
            tokenAddress: address(0),
            decimals: 0,
            evmAmount: 1_000_000,
            cosmosAmount: 0,
            expectedRevertSelector: abi.encodeWithSelector(ZeroAddress.selector, address(0))
        });

        testCases[1] = RevertTestCase({
            m: "Zero address for SdkCoin to ERC20 conversion",
            tokenAddress: address(0),
            decimals: 0,
            evmAmount: 0,
            cosmosAmount: 1_000_000,
            expectedRevertSelector: abi.encodeWithSelector(ZeroAddress.selector, address(0))
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
            expectedRevertSelector: abi.encodeWithSelector(ZeroAmountUint64.selector, uint64(0))
        });
        testCases[5] = RevertTestCase({
            m: "Zero amount for ERC20 to SdkCoin conversion",
            tokenAddress: address(new MockERC20Metadata(6)),
            decimals: 6,
            evmAmount: 0,
            cosmosAmount: 0,
            expectedRevertSelector: abi.encodeWithSelector(ZeroAmountUint256.selector, uint256(0))
        });

        for (uint256 i = 0; i < testCases.length; i++) {
            RevertTestCase memory tc = testCases[i];

            vm.expectRevert(tc.expectedRevertSelector);
            if (tc.evmAmount != 0) {
                SdkCoin._convertERC20AmountToSdkCoin(tc.tokenAddress, tc.evmAmount);
            } else {
                SdkCoin._convertSdkCoinAmountToERC20(tc.tokenAddress, tc.cosmosAmount);
            }
        }
    }

    /*
    We now provide the invariant testing for the conversion functions, which includes fuzz testing. In our testing
    approach, we focus on invariant testing, which offers several benefits:

    - Fuzz testing, as part of invariant testing, generates random inputs to test the functions, helping uncover
    unexpected bugs and edge cases.
    - Invariant testing ensures that core properties and conditions that must always hold true for the function's
    outputs are consistently upheld, providing more meaningful and robust validation.

    This combined approach ensures we can effectively test without manually computing expected values for each input. 
    It guarantees that the core properties of the conversion logic are maintained, leading to more reliable and accurate 
    validation of the conversion functions.
    */

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

    // Invariants Tests
    // Invariant test: ERC20 -> SdkCoin with tokenDecimals == DEFAULT_COSMOS_DECIMALS,
    // the converted amount should always be == to the input amount, and the remainder should be 0.
    /**
     * @notice Invariant test for ERC20 to Cosmos coin conversion when token decimals are equal to Cosmos decimals
     * @notice Given random inputs, this test applies invariant checking to ensure the converted amount equals the input
     * amount and the remainder is zero
     * @param amount The amount of ERC20 tokens to be converted
     */
    function testInvariant_ERC20toSdkCoin_EqualDecimals(uint256 amount) public {
        // Skip test for zero amount or overflow conditions
        if (amount == 0 || amount > ~uint64(0)) {
            // These conditions will revert and are expected behaviours that have already been covered in table testing
            return;
        }
        address tokenAddress = address(new MockERC20Metadata(6));
        (uint64 convertedAmount, uint256 remainder) =
            SdkCoin._convertERC20AmountToSdkCoin(address(tokenAddress), amount);
        assertInvariants_ERC20toSdkCoin_EqualDecimals(convertedAmount, amount, remainder);
    }

    // Invariant test: In the case ERC20 -> SdkCoin with tokenDecimals > DEFAULT_COSMOS_DECIMALS
    // the converted amount should always be <= to the input amount. If less, the remainder should be > 0.
    /**
     * @notice Invariant test for ERC20 to Cosmos coin conversion with token decimals greater than Cosmos decimals
     * @notice Given random inputs, this test applies invariant checking previously defined
     * @param amount The amount of ERC20 tokens to be converted
     * @param decimals The number of decimals of the ERC20 token
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
            SdkCoin._convertERC20AmountToSdkCoin(address(tokenAddress), amount);
        assertInvariants_ERC20toSdkCoin_BiggerDecimals(convertedAmount, amount, remainder, factor);
    }

    // Invariant test: In the case SdkCoin -> ERC20 the converted amount should always be >= to the input amount.
    function testInvariant_SdkCoinToERC20(uint64 amount, uint8 decimals) public {
        // Inputs constraints
        if (amount == 0 || amount > ~uint64(0) || decimals < 6 || decimals > 77) {
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

        uint256 convertedAmount = SdkCoin._convertSdkCoinAmountToERC20(address(tokenAddress), amount);
        invariant_SdkCointoERC20_ConvertedAmountBiggerOrEqualThanInput(convertedAmount, amount);
    }
}
