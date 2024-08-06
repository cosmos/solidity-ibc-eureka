// SPDX-License-Identifier: MIT
pragma solidity >=0.8.25;

// https://docs.openzeppelin.com/contracts/4.x/api/token/erc20#IERC20Metadata
import { IERC20Metadata } from "@openzeppelin/contracts/token/ERC20/extensions/IERC20Metadata.sol";

library ERC20CosmosCoinAmountConversion {
    // Using Constants for decimals
    uint8 constant DEFAULT_ERC20_DECIMALS = 18;
    // https://docs.cosmos.network/v0.50/build/architecture/adr-024-coin-metadata
    // TODO Do we want to support flexibility for cosmos coin decimals?
    uint8 constant DEFAULT_COSMOS_DECIMALS = 6;

    // Note that ERC20 standard use 18 decimals by default. Custom ERC20 implementation may decide to change this.
    /**
     * @notice Gets the decimals of a token if it extends the IERC20 standard using IERC20Metadata
     * @param tokenAddress The address of the token contract
     * @return decimals The number of decimals of the token
     */
    function _getERC20TokenDecimals(address tokenAddress) internal view returns (uint8) {
        // Input validation
        // TODO Add custom error?
        require(tokenAddress != address(0), "Address cannot be the zero address");
        // TODO Reason about this check - Remember it as a good practice but what real protections give us?
        // TODO Add custom error?
        require(tokenAddress != address(this), "Address cannot be the contract itself");

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

    function _convertERC20AmountToCosmosCoin(
        address tokenAddress,
        uint256 amount
    )
        internal
        view
        returns (uint64 convertedAmount, uint256 remainder)
    {
        // Note if we want to allow different cosmos decimals, we need to handle that here too
        // Note that tokenAddress input validations are executed in the _getERC20TokenDecimals function
        uint8 tokenDecimals = _getERC20TokenDecimals(tokenAddress);
        // TODO write an ADR for this?
        require(tokenDecimals >= 6, "ERC20 Tokens with less than 6 decimals are not supported");
        // TODO Add custom error?
        // Amount input validation
        require(amount != 0, "Requested conversion for the 0 amount");
        // Ensure the amount respects the token's decimals
        // Handle the case where the input amount exceeds the token's precision
        uint256 temp_convertedAmount;
        uint256 factor;
        // Case ERC20 decimals are bigger than cosmos decimals
        if (tokenDecimals > DEFAULT_COSMOS_DECIMALS) {
            factor = 10 ** (tokenDecimals - DEFAULT_COSMOS_DECIMALS);
            temp_convertedAmount = amount / factor;
            remainder = amount % factor;
        } else if (tokenDecimals < DEFAULT_COSMOS_DECIMALS) {
            // Keeping this code here until final team decision is made.
            // Restricting the support for decimals >= 6 this part will never be executed.
            // Case ERC20 decimals < DEFAULT_COSMOS_DECIMALS
            // Note we need to amplify the decimals
            factor = 10 ** (DEFAULT_COSMOS_DECIMALS - tokenDecimals);
            temp_convertedAmount = amount * factor;
            remainder = 0;
        } else {
            // Case ERC20 decimals == DEFAULT_COSMOS_DECIMALS --> Amount will fit the uint64
            // Note that we need to handle the case < and == differently
            temp_convertedAmount = amount;
            remainder = 0;
        }
        // TODO Add custom error?
        // ~uint64(0) is the max value supported by uint64
        require(temp_convertedAmount <= ~uint64(0), "Converted amount exceeds uint64 limits");
        // At this point we should be sure that we are not loosing precision
        convertedAmount = uint64(temp_convertedAmount);
        return (convertedAmount, remainder);
    }

    // Convert Cosmos coin amount to ERC20 token amount
    // Assuming that we support only ERC20.decimlas()>=6
    /**
     * @notice Convert the uint64 Cosmos coin amount into ERC20 token amount uint256
     * @param tokenAddress The address of the token contract
     * @param amount The amount to be converted
     * @return convertedAmount The amount converted to uint256 supported by ERC20 tokens
     */
    function _convertCosmosCoinAmountToERC20(
        uint64 amount,
        address tokenAddress
    )
        internal
        view
        returns (uint256 convertedAmount)
    {
        // Get the token decimals
        // address input validation perfomed in the _getERC20TokenDecimals
        uint8 tokenDecimals = _getERC20TokenDecimals(tokenAddress);

        // Ensure the token has at least 6 decimals
        require(tokenDecimals >= 6, "ERC20 Tokens with less than 6 decimals are not supported");
        // Amount is not 0
        require(amount != 0, "Requested conversion for the 0 amount");
        uint256 factor;

        // Case ERC20 decimals are bigger than cosmos decimals
        if (tokenDecimals > DEFAULT_COSMOS_DECIMALS) {
            factor = 10 ** (tokenDecimals - DEFAULT_COSMOS_DECIMALS);
            // uint256 = uint64 * uint256 that should be ok
            convertedAmount = amount * factor;
        } else if (tokenDecimals < DEFAULT_COSMOS_DECIMALS) {
            // TODO if we decide to support this case. It will require handling the loss of precision
            // in the go side
        } else {
            // Case ERC20 decimals == DEFAULT_COSMOS_DECIMALS
            convertedAmount = amount;
        }

        return convertedAmount;
    }
}
