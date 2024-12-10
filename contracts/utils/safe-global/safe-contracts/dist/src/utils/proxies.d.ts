import { Contract, BigNumberish } from "ethers";
export declare const calculateProxyAddress: (factory: Contract, singleton: string, inititalizer: string, nonce: number | string) => Promise<string>;
export declare const calculateProxyAddressWithCallback: (factory: Contract, singleton: string, inititalizer: string, nonce: number | string, callback: string) => Promise<string>;
export declare const calculateChainSpecificProxyAddress: (factory: Contract, singleton: string, inititalizer: string, nonce: number | string, chainId: BigNumberish) => Promise<string>;
