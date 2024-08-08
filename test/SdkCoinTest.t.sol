// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25 <0.9.0;

// solhint-disable custom-errors,max-line-length
import { Test } from "forge-std/Test.sol";
import { SdkCoin } from "../src/utils/SdkCoin.sol";
import { ERC20 } from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import { IISdkCoinErrors } from "../src/errors/IISdkCoinErrors.sol";
import { SafeCast } from "@openzeppelin/contracts/utils/math/SafeCast.sol";

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
            expectedRevertSelector: abi.encodeWithSelector(IISdkCoinErrors.ZeroAddress.selector, address(0))
        });

        testCases[1] = RevertTestCase({
            m: "Zero address for SdkCoin to ERC20 conversion",
            tokenAddress: address(0),
            decimals: 0,
            evmAmount: 0,
            cosmosAmount: 1_000_000,
            expectedRevertSelector: abi.encodeWithSelector(IISdkCoinErrors.ZeroAddress.selector, address(0))
        });

        testCases[2] = RevertTestCase({
            m: "Less than six decimals for ERC20 to SdkCoin conversion",
            tokenAddress: address(new MockERC20Metadata(5)),
            decimals: 5,
            evmAmount: 1_000_000,
            cosmosAmount: 0,
            expectedRevertSelector: abi.encodeWithSelector(IISdkCoinErrors.UnsupportedTokenDecimals.selector, uint8(5))
        });

        testCases[3] = RevertTestCase({
            m: "Less than six decimals for SdkCoin to ERC20 conversion",
            tokenAddress: address(new MockERC20Metadata(5)),
            decimals: 5,
            evmAmount: 0,
            cosmosAmount: 1_000_000,
            expectedRevertSelector: abi.encodeWithSelector(IISdkCoinErrors.UnsupportedTokenDecimals.selector, uint8(5))
        });

        testCases[4] = RevertTestCase({
            m: "Zero amount for SdkCoin to ERC20 conversion",
            tokenAddress: address(new MockERC20Metadata(6)),
            decimals: 6,
            evmAmount: 0,
            cosmosAmount: 0,
            expectedRevertSelector: abi.encodeWithSelector(IISdkCoinErrors.ZeroAmountUint64.selector, uint64(0))
        });

        testCases[5] = RevertTestCase({
            m: "Zero amount for ERC20 to SdkCoin conversion",
            tokenAddress: address(new MockERC20Metadata(6)),
            decimals: 6,
            evmAmount: 0,
            cosmosAmount: 0,
            expectedRevertSelector: abi.encodeWithSelector(IISdkCoinErrors.ZeroAmountUint256.selector, uint256(0))
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
    These functions convert ERC20 token amount <-> Cosmos SDK coin amount, taking into account the ERC20 token's decimals.
    This ensures that the amount adheres to the token's precision and handles various cases where the ERC20 token's decimals differ
    from the default Cosmos decimals.

    For this specific context, fuzz testing should be only used in combination with invariant testing. Here’s why:

    Fuzz testing involves generating random inputs to test the function, which can help uncover unexpected bugs. However, for these
    specific functions, fuzz testing alone is less effective due to the following reasons:
    1. Computing the expected values for these conversions requires manual thinking. Using a standard conversion function 
       to calculate expected values for random inputs negates the need for this library and makes the 
       fuzz testing process pointless.
    2. Directly replicating the function's logic in tests to calculate expected outputs results in tautological testing. 
       This means the test simply mirrors the implementation, providing no real verification and thus no added value.
    3. Fuzz testing could be eventually useful to trigger failure scenarios, but these are already covered by table testing. 

    Instead, combining fuzz testing with invariant testing is more appropriate since it focuses on properties and conditions 
    that must always hold true for the function's outputs. This approach is more meaningful for validation of these functions 
    as it does not require computing expected values.
    
    By focusing on invariants, we can ensure that the core properties of the conversion logic are always upheld, providing stronger
    guarantees of correctness. Overall, this combined approach ensures thorough testing by leveraging the strengths of 
    both fuzz testing and invariant testing, leading to more reliable and accurate validation of the conversion functions.

    Example invariant properties that we may use are: 
    - If `tokenDecimals == DEFAULT_COSMOS_DECIMALS`, the converted amount should equal the input amount, and 
      the remainder should be 0.
    - In the case ERC20 -> SdkCoin the converted amount should always be <= to the input amount. If less 
      the remainder should be > 0.
    - In the case SdkCoin -> ERC20 the converted amount should always be >= to the input amount. 
    - For non-zero inputs, the remainder should be less than the factor used for conversion. (e.g., the divisor).
*/

    
 // Invariant test: If `tokenDecimals == DEFAULT_COSMOS_DECIMALS`, the converted amount should equal the input amount, and the remainder should be 0.
function testInvariantEqualDecimals(uint256 amount) public {
    // Skip test for zero amount or overflow conditions
    uint64 maxUint64 = type(uint64).max;
    if (amount == 0 || amount > maxUint64) {
        return;
    }

    address tokenAddress = address(new MockERC20Metadata(6));
    uint8 tokenDecimals = SdkCoin._getERC20TokenDecimals(address(tokenAddress));

    if (tokenDecimals == SdkCoin.DEFAULT_COSMOS_DECIMALS) {
        (uint64 convertedAmount, uint256 remainder) = SdkCoin._convertERC20AmountToSdkCoin(address(tokenAddress), amount);
        assertEq(convertedAmount, SafeCast.toUint64(amount), "Converted amount should equal input amount");
        assertEq(remainder, 0, "Remainder should be 0");
    }
}

// Invariant test: In the case ERC20 -> SdkCoin the converted amount should always be <= to the input amount. If less, the remainder should be > 0.
function testInvariantERC20ToSdkCoin(uint256 amount) public {
    // Skip test for zero amount or overflow conditions
    uint64 maxUint64 = type(uint64).max;
    if (amount == 0 || amount > maxUint64) {
        return;
    }

    address tokenAddress = address(new MockERC20Metadata(6));

    (uint64 convertedAmount, uint256 remainder) = SdkCoin._convertERC20AmountToSdkCoin(address(tokenAddress), amount);
    assertTrue(convertedAmount <= amount, "Converted amount should be <= input amount");
    if (convertedAmount < amount) {
        assertTrue(remainder > 0, "Remainder should be > 0");
    }
}

// Invariant test: In the case SdkCoin -> ERC20 the converted amount should always be >= to the input amount.
function testInvariantSdkCoinToERC20(uint64 amount) public {
    // Skip test for zero amount
    if (amount == 0) {
        return;
    }

    address tokenAddress = address(new MockERC20Metadata(18));

    uint256 convertedAmount = SdkCoin._convertSdkCoinAmountToERC20(address(tokenAddress), amount);
    assertTrue(convertedAmount >= amount, "Converted amount should be >= input amount");
}

// Invariant test: For non-zero inputs, the remainder should be less than the factor used for conversion.
function testInvariantRemainder(uint256 amount) public {
    // Skip test for zero amount or overflow conditions
    uint64 maxUint64 = type(uint64).max;
    if (amount == 0 || amount > maxUint64) {
        return;
    }

    address tokenAddress = address(new MockERC20Metadata(6));

    uint8 tokenDecimals = SdkCoin._getERC20TokenDecimals(address(tokenAddress));
    if (tokenDecimals > SdkCoin.DEFAULT_COSMOS_DECIMALS) {
        (uint64 convertedAmount, uint256 remainder) = SdkCoin._convertERC20AmountToSdkCoin(address(tokenAddress), amount);
        uint256 factor = 10 ** (tokenDecimals - SdkCoin.DEFAULT_COSMOS_DECIMALS);
        assertTrue(remainder < factor, "Remainder should be less than conversion factor");
    }
}



}
