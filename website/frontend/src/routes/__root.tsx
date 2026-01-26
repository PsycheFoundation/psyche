import { Outlet, createRootRoute } from "@tanstack/react-router";
import { darkTheme, lightTheme, sharedTheme } from "../themes.js";
import { useDarkMode } from "usehooks-ts";
import { css } from "@linaria/core";
import { c } from "../utils.js";
import {
  ConnectionProvider,
  WalletProvider,
} from "@solana/wallet-adapter-react";
import { WalletModalProvider } from "@solana/wallet-adapter-react-ui";

import "@solana/wallet-adapter-react-ui/styles.css";

export const Route = createRootRoute({
  component: RootComponent,
});

const fullHeight = css`
  min-height: 100vh;
`;

if (
  !import.meta.env.VITE_MINING_POOL_RPC ||
  !import.meta.env.VITE_MINING_POOL_RPC.startsWith("http")
) {
  throw new Error(
    `Invalid deployment config. env var VITE_MINING_POOL_RPC was not set when building.`,
  );
}

if (!import.meta.env.VITE_COORDINATOR_CLUSTER) {
  throw new Error(
    `Invalid deployment config. env var VITE_COORDINATOR_CLUSTER was not set when building.`,
  );
}
if (!import.meta.env.VITE_MINING_POOL_CLUSTER) {
  throw new Error(
    `Invalid deployment config. env var VITE_MINING_POOL_CLUSTER was not set when building.`,
  );
}

function RootComponent() {
  const { isDarkMode } = useDarkMode();

  return (
    <ConnectionProvider endpoint={import.meta.env.VITE_MINING_POOL_RPC}>
      <WalletProvider wallets={[]} onError={(err) => console.error(err)}>
        <WalletModalProvider>
          <div
            id="outlet"
            className={`${fullHeight} ${sharedTheme} ${isDarkMode ? c(darkTheme, "theme-dark") : c(lightTheme, "theme-light")}`}
          >
            <Outlet />
          </div>
        </WalletModalProvider>
      </WalletProvider>
    </ConnectionProvider>
  );
}
