import {
    airdropFactory,
    appendTransactionMessageInstructions,
    assertIsTransactionWithinSizeLimit,
    createSolanaRpc,
    createSolanaRpcSubscriptions,
    createTransactionMessage,
    generateKeyPairSigner,
    getSignatureFromTransaction,
    lamports,
    pipe,
    sendAndConfirmTransactionFactory,
    setTransactionMessageFeePayerSigner,
    setTransactionMessageLifetimeUsingBlockhash,
    signTransactionMessageWithSigners
} from '@solana/kit';
import { getCreateAccountInstruction } from '@solana-program/system';
import { getInitializeMintInstruction, getMintSize, TOKEN_PROGRAM_ADDRESS } from '@solana-program/token';

const rpc = createSolanaRpc("http://localhost:8899");
const rpcSubscriptions= createSolanaRpcSubscriptions("ws://localhost:8900");

async function main() {
    const feePayer = await generateKeyPairSigner();

    await airdropFactory({rpc, rpcSubscriptions})({
        recipientAddress: feePayer.address,
        lamports: lamports(1_000_000_000n),
        commitment: "confirmed"
    });

    const mint = await generateKeyPairSigner();

    const space = BigInt(getMintSize());

    const rentExemption = await rpc.getMinimumBalanceForRentExemption(space).send();

    const createAccountIx = getCreateAccountInstruction({
        payer: feePayer,
        newAccount: mint,
        lamports: rentExemption,
        space,
        programAddress: TOKEN_PROGRAM_ADDRESS
    });

    const initializeMintIx = getInitializeMintInstruction({
        mint: mint.address,
        decimals: 9,
        mintAuthority: feePayer.address
    });

    const ixs = [createAccountIx, initializeMintIx];

    const { value: blockhash } = await rpc.getLatestBlockhash().send();

    const tx = pipe(
        createTransactionMessage({ version: 0 }),
        (tx) => setTransactionMessageFeePayerSigner(feePayer, tx),
        (tx) => setTransactionMessageLifetimeUsingBlockhash(blockhash, tx),
        (tx) => appendTransactionMessageInstructions(ixs, tx)
    );

    const sign = await signTransactionMessageWithSigners(tx);
    
    assertIsTransactionWithinSizeLimit(sign);

    await sendAndConfirmTransactionFactory({rpc, rpcSubscriptions})(
        sign, 
        { commitment: "confirmed" }
    );

    const txSignature = getSignatureFromTransaction(sign);

    console.log("Signature is: ", txSignature);
    console.log("Mint is: ", mint);
}

main();
