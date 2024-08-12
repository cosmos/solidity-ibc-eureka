// SPDX-License-Identifier: MIT
pragma solidity >=0.8.25;

// https://docs.openzeppelin.com/contracts/4.x/api/token/erc20#IERC20Metadata
import { IERC20Metadata } from "@openzeppelin/contracts/token/ERC20/extensions/IERC20Metadata.sol";
import { SafeCast } from "@openzeppelin/contracts/utils/math/SafeCast.sol";
import { IISdkCoinErrors } from "../errors/IISdkCoinErrors.sol";

library SdkCoin {
    /// @notice Default value for ERC20 decimals if the token does not implement the decimals() function
    uint8 public constant DEFAULT_ERC20_DECIMALS = 18;
    /// @notice Default value for Cosmos coin decimals
    /// @dev https://docs.cosmos.network/v0.50/build/architecture/adr-024-coin-metadata
    uint8 public constant DEFAULT_COSMOS_DECIMALS = 6;

    // Note that ERC20 standard use 18 decimals by default. Custom ERC20 implementation may decide to change this.
    /**
     * @notice Gets the decimals of a token if it extends the IERC20 standard using IERC20Metadata
     * @param tokenAddress The address of the token contract
     * @return decimals The number of decimals of the token
     */
    function _getERC20TokenDecimals(address tokenAddress) internal view returns (uint8) {
        // Input validation
        if (tokenAddress == address(0)) {
            revert IISdkCoinErrors.InvalidAddress(tokenAddress);
        }
        // If the tokens extends the IERC20 it should be using IERC20Metadata which supports the decimals() call
        // Why this? -->  https://detectors.auditbase.com/decimals-erc20-standard-solidity
        try IERC20Metadata(tokenAddress).decimals() returns (uint8 decimals) {
            return decimals;
        } catch {
            // If the decimals function is not implemented, return the default value
            return DEFAULT_ERC20_DECIMALS;
        }
    }

    // Convert ERC20 tokens decimals to Cosmos decimals units
    // NOTE that the refund to the user of the remainder of the conversion should happen during the transfer.
    /**
     * @notice Convert the uint256 ERC20 token amount into cosmos coin amount uint64
     * @notice This functions allows us to consider the loss of precision
     * @param tokenAddress The address of the token contract
     * @param amount The amount to be converted
     * @return convertedAmount The amount converted to uint64 supported by cosmos coins
     * @return remainder The remainder of the conversion
     */
    function _ERC20ToSdkCoin_ConvertAmount(
        address tokenAddress,
        uint256 amount
    )
        internal
        view
        returns (uint64, uint256)
    {
        // Retrieve the number of decimals the ERC20 token uses
        // Note that input validations for the tokenAddress are performed within _getERC20TokenDecimals
        uint8 tokenDecimals = _getERC20TokenDecimals(tokenAddress);

        // Ensure the token's decimals are within the supported range
        if (tokenDecimals < 6 || tokenDecimals > 77) {
            revert IISdkCoinErrors.UnsupportedTokenDecimals(tokenDecimals);
        }

        // Ensure the provided amount is not zero, as zero is an invalid amount
        if (amount == 0) {
            revert IISdkCoinErrors.InvalidAmount(amount);
        }

        // Note that for values of amount > ~uint64(0) which is the max value of uint64 type
        // The function will revert automatically for the safeCast conversion function. No need to check it here.

        // Variables to store the converted amount and any remainder
        uint256 tempConvertedAmount;
        uint256 remainder;

        // Case where ERC20 token decimals are greater than the default cosmos decimals
        if (tokenDecimals > DEFAULT_COSMOS_DECIMALS) {
            // Calculate the factor by which to scale down the amount
            uint256 factor = 10 ** (tokenDecimals - DEFAULT_COSMOS_DECIMALS);

            // Solidity version > 0.8 includes built-in overflow/underflow checks
            // Scale down the amount by the factor to adjust for the difference in decimals
            tempConvertedAmount = amount / factor;

            // Calculate any remainder that cannot be represented in the target decimal format
            remainder = amount % factor;
        } else if (tokenDecimals == DEFAULT_COSMOS_DECIMALS) {
            // If the token decimals match the cosmos decimals, no conversion is necessary
            tempConvertedAmount = amount;
            remainder = 0;
        } else {
            // This case should be unreachable because of the earlier check for minimum token decimals
            revert IISdkCoinErrors.Unsupported();
        }

        // Return the converted amount cast to uint64 and any remainder
        return (SafeCast.toUint64(tempConvertedAmount), remainder);
    }

    // Convert Cosmos coin amount to ERC20 token amount
    // Assuming that we support only ERC20.decimlas()>=6
    /**
     * @notice Convert the uint64 Cosmos coin amount into ERC20 token amount uint256
     * @param tokenAddress The address of the token contract
     * @param amount The amount to be converted
     * @return convertedAmount The amount converted to uint256 supported by ERC20 tokens
     */
    function _SdkCoinToERC20_ConvertAmount(address tokenAddress, uint64 amount) internal view returns (uint256) {
        // Get the token decimals
        // address input validation perfomed in the _getERC20TokenDecimals
        uint8 tokenDecimals = _getERC20TokenDecimals(tokenAddress);

        // Ensure the token has at least 6 decimals and max 77
        if (tokenDecimals < 6 || tokenDecimals > 77) {
            revert IISdkCoinErrors.UnsupportedTokenDecimals(tokenDecimals);
        }
        // Amount input validation
        if (amount == 0) {
            revert IISdkCoinErrors.InvalidAmount(amount);
        }
        uint256 convertedAmount;
        // Case ERC20 decimals are bigger than cosmos decimals
        if (tokenDecimals > DEFAULT_COSMOS_DECIMALS) {
            uint256 factor = 10 ** (tokenDecimals - DEFAULT_COSMOS_DECIMALS);
            // uint256 = uint64 * uint256 that should be ok
            // Solidity version > 0.8 includes built in overflows/underflows checks
            // This multiplication will overflow if amount > (~uint256(0) / factor)
            convertedAmount = amount * factor;
        } else if (tokenDecimals == DEFAULT_COSMOS_DECIMALS) {
            // uint256 = uint64 should be ok
            convertedAmount = amount;
        } else {
            // Case ERC20 decimals < DEFAULT_COSMOS_DECIMALS
            // Note that if we decide to support this case, it will require handling the loss of precision
            // in the go side. Potentially we can retrieve the ERC20 decimals using cross-chain queries

            // revert as this is unreachable
            revert IISdkCoinErrors.Unsupported();
        }

        return convertedAmount;
    }
}
