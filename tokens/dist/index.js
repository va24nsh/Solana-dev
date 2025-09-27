"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
const kit_1 = require("@solana/kit");
const system_1 = require("@solana-program/system");
const token_1 = require("@solana-program/token");
const rpc = (0, kit_1.createSolanaRpc)("http://localhost:8899");
const rpcSubscriptions = (0, kit_1.createSolanaRpcSubscriptions)("ws://localhost:8900");
async function main() {
    const feePayer = await (0, kit_1.generateKeyPairSigner)();
    await (0, kit_1.airdropFactory)({ rpc, rpcSubscriptions })({
        recipientAddress: feePayer.address,
        lamports: (0, kit_1.lamports)(1000000000n),
        commitment: "confirmed"
    });
    const mint = await (0, kit_1.generateKeyPairSigner)();
    const space = BigInt((0, token_1.getMintSize)());
    const rentExemption = await rpc.getMinimumBalanceForRentExemption(space).send();
    const createAccountIx = (0, system_1.getCreateAccountInstruction)({
        payer: feePayer,
        newAccount: mint,
        lamports: rentExemption,
        space,
        programAddress: token_1.TOKEN_PROGRAM_ADDRESS
    });
    const initializeMintIx = (0, token_1.getInitializeMintInstruction)({
        mint: mint.address,
        decimals: 9,
        mintAuthority: feePayer.address
    });
    const ixs = [createAccountIx, initializeMintIx];
    const { value: blockhash } = await rpc.getLatestBlockhash().send();
    const tx = (0, kit_1.pipe)((0, kit_1.createTransactionMessage)({ version: 0 }), (tx) => (0, kit_1.setTransactionMessageFeePayerSigner)(feePayer, tx), (tx) => (0, kit_1.setTransactionMessageLifetimeUsingBlockhash)(blockhash, tx), (tx) => (0, kit_1.appendTransactionMessageInstructions)(ixs, tx));
    const sign = await (0, kit_1.signTransactionMessageWithSigners)(tx);
    (0, kit_1.assertIsTransactionWithinSizeLimit)(sign);
    await (0, kit_1.sendAndConfirmTransactionFactory)({ rpc, rpcSubscriptions })(sign, { commitment: "confirmed" });
    const txSignature = (0, kit_1.getSignatureFromTransaction)(sign);
    console.log("Signature is: ", txSignature);
    console.log("Mint is: ", mint);
}
main();
