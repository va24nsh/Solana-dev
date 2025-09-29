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
} from "@solana/kit";
import { getCreateAccountInstruction } from "@solana-program/system";
import { 
    getInitializeAccount2Instruction, 
    getInitializeMintInstruction, 
    getMintSize, 
    getTokenSize, 
    TOKEN_PROGRAM_ADDRESS 
} from "@solana-program/token";

const rpc = createSolanaRpc("http://localhost:8899");
const rpcSubscriptions = createSolanaRpcSubscriptions("ws://localhost:8900");

async function main() {
    // Get the latest blockhash
    const { value: blockhash } = await rpc.getLatestBlockhash().send()

    // Generate a FeePayer / YOU
    const feePayer = await generateKeyPairSigner();

    // Airdrop the payer to fund the accounts
    await airdropFactory({ rpc, rpcSubscriptions })({
        recipientAddress: feePayer.address,
        lamports: lamports(1_000_000_000n),
        commitment: "confirmed"
    })

    // Generate key pair for mint account
    const mint = await generateKeyPairSigner();

    // Calculate the space for creating the mint account
    const space = BigInt(getMintSize());

    // Calculate rent exemption
    const rent = await rpc.getMinimumBalanceForRentExemption(space).send();

    // Create account using System Program and then transfer ownership
    const createMintAccount = getCreateAccountInstruction({
        payer: feePayer,
        newAccount: mint,
        space,
        lamports: rent,
        programAddress: TOKEN_PROGRAM_ADDRESS
    });

    // Initialize using token program
    const initializeMintAccount = getInitializeMintInstruction({
        mint: mint.address,
        decimals: 2,
        mintAuthority: feePayer.address
    });

    const ixs = [createMintAccount, initializeMintAccount];

    const txs = pipe(
        createTransactionMessage({ version: 0 }),
        tx => setTransactionMessageFeePayerSigner(feePayer, tx),
        tx => setTransactionMessageLifetimeUsingBlockhash(blockhash, tx),
        tx => appendTransactionMessageInstructions(ixs, tx)
    );

    const signed = await signTransactionMessageWithSigners(txs);

    assertIsTransactionWithinSizeLimit(signed);

    await sendAndConfirmTransactionFactory({ rpc, rpcSubscriptions })(
        signed,
        { commitment: "confirmed" }
    );

    const transactionSignature = getSignatureFromTransaction(signed);

    console.log("Transaction 1 Signature: ", transactionSignature);

    // Generate key pair for token account
    const token = await generateKeyPairSigner();

    const tokenSpace = BigInt(getTokenSize());

    const tokenRent = await rpc.getMinimumBalanceForRentExemption(tokenSpace).send();

    const createTokenAccount = getCreateAccountInstruction({
        payer: feePayer,
        newAccount: token,
        lamports: tokenRent,
        space: tokenSpace,
        programAddress: TOKEN_PROGRAM_ADDRESS
    });

    const initializeTokenAccount = getInitializeAccount2Instruction({
        account: token.address,
        mint: mint.address,
        owner: feePayer.address
    });

    const ixs2 = [createTokenAccount, initializeTokenAccount];

    const { value: latestBlockhash } = await rpc.getLatestBlockhash().send();

    const txs2 = pipe(
        createTransactionMessage({ version: 0 }),
        tx => setTransactionMessageFeePayerSigner(feePayer, tx),
        tx => setTransactionMessageLifetimeUsingBlockhash(latestBlockhash, tx),
        tx => appendTransactionMessageInstructions(ixs2, tx)
    );

    const signed2 = await signTransactionMessageWithSigners(txs2);

    assertIsTransactionWithinSizeLimit(signed2);

    await sendAndConfirmTransactionFactory({ rpc, rpcSubscriptions })(
        signed2,
        { commitment: "confirmed" }
    );

    const signature2 = getSignatureFromTransaction(signed2);

    console.log("Transaction 2 : ", signature2);
}

main();