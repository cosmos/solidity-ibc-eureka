// SPDX-License-Identifier: MIT
pragma solidity >=0.8.25;

import { Strings } from "@openzeppelin/contracts/utils/Strings.sol";
import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
// https://docs.openzeppelin.com/contracts/4.x/api/token/erc20#IERC20Metadata
import "@openzeppelin/contracts/token/ERC20/extensions/IERC20Metadata.sol";

library ERC20CosmosCoinConversion {
    // Using Constants for decimals
    uint8 constant DEFAULT_ERC20_DECIMALS = 18; 
    // https://docs.cosmos.network/v0.50/build/architecture/adr-024-coin-metadata
    // TODO Do we want to support flexibility for cosmos coin decimals?  
    uint8 constant DEFAULT_COSMOS_DECIMALS = 6;

    // Note that ERC20 standard use 18 decimals by default. Custom ERC20 implementation may decide to change this. 
    // TODO revisit this to see if we missed something important 
    // Potential problems that may arise: https://detectors.auditbase.com/decimals-erc20-standard-solidity
    /**
     * @notice Gets the decimals of a token if it extends the IERC20 standard using IERC20Metadata
     * @param tokenAddress The address of the token contract
     * @return decimals The number of decimals of the token
     */
    function _getERC20TokenDecimals(address tokenAddress) internal view returns (uint8) {
        // Input validation 
        // TODO Add custom error?  
        require(tokenAddress!= address(0), "Address cannot be the zero address"); 
        // TODO Reason about this check - Remember it as a good practice but what real protections give us? 
        // TODO Add custom error?  
        require(tokenAddress!= address(this), "Address cannot be the contract itself");
        
        // If the tokens extends the IERC20 it should be using IERC20Metadata which supports the decimals() call 
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
    ) internal view returns (uint64 convertedAmount, uint256 remainder) {
        // Note if we want to allow different cosmos decimals, we need to handle that here too 
        // Note that tokenAddress input validations are executed in the _getERC20TokenDecimals function  
        uint8 tokenDecimals = _getERC20TokenDecimals(tokenAddress); 
        // TODO Add custom error?  
        // Amount input validation
        require(amount!=0, "Requested conversion for the 0 amount"); 
        // Ensure the amount respects the token's decimals

        uint256 temp_convertedAmount;
        uint256 factor;  
        // Case ERC20 decimals are bigger than cosmos decimals
        if(tokenDecimals > DEFAULT_COSMOS_DECIMALS){
            factor = 10 ** (tokenDecimals - DEFAULT_COSMOS_DECIMALS);
            temp_convertedAmount = amount / factor;
            remainder = amount % factor;
        }
        else{
            // Case ERC20 decimals <= DEFAULT_COSMOS_DECIMALS 
            // Note that we need to handle the case < and == differently 
            if(tokenDecimals < DEFAULT_COSMOS_DECIMALS){
            // Note we need to amplify the decimals 
            factor = 10 ** (DEFAULT_COSMOS_DECIMALS - tokenDecimals);
            temp_convertedAmount = amount * factor;
            remainder=0;
            }
            else {
            // Case ERC20 decimals == DEFAULT_COSMOS_DECIMALS --> Amount will fit the uint64
            temp_convertedAmount = amount; 
            remainder=0;
            }
        } 
        // TODO Add custom error?  
        // ~uint64(0) is the max value supported by uint64 
        require(temp_convertedAmount <= ~uint64(0), "Converted amount exceeds uint64 limits");
        // At this point we should be sure that we are not loosing precision 
        convertedAmount=uint64(temp_convertedAmount);
        return (convertedAmount, remainder);
    }


    // TODO Rework considering the flexibility of ERC20 decimals 
    // Convert Cosmos coin amount to ERC20 token amount
    function _convertCosmosCoinAmountToERC20(
        uint64 amount
    ) internal pure returns (uint256) {
        uint256 factor = 10 ** (DEFAULT_ERC20_DECIMALS - DEFAULT_COSMOS_DECIMALS);
        return amount * factor;
    }

/* TODOs.. 
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
        uint32 denomUnitsCoin, //  Consider creating a struct here. 
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
*/
}