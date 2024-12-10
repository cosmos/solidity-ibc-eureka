import { Contract } from "ethers";
import { MetaTransaction, SafeTransaction } from "./execution";
export declare const encodeMultiSend: (txs: MetaTransaction[]) => string;
export declare const buildMultiSendSafeTx: (multiSend: Contract, txs: MetaTransaction[], nonce: number, overrides?: Partial<SafeTransaction>) => SafeTransaction;
