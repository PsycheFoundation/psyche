import { PublicKey } from "@solana/web3.js";
import { ToolboxEndpoint } from "solana_toolbox_web3";
import { monitorTransactions } from "./monitor";

const endpoint = new ToolboxEndpoint("devnet", "confirmed");

async function main() {
  console.log("Hello, Solana Indexer!");
  monitorTransactions(endpoint, PublicKey.default, (signature, execution) => {
    console.log("------");
    //console.log("Transaction:", signature, execution);
  });
  console.log("After sync call");
}

main();
