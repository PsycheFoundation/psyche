import { TxSummary } from "shared";
import { SolanaCluster, solanaTxUrl } from "../utils.js";
import { styled } from "@linaria/react";
import { slate, forest } from "../colors.js";
import { text } from "../fonts.js";

const TxsContainer = styled.div`
  line-height: 1.3em;
  margin: 16px 16px;
  padding: 0;
  overflow: scroll;
  height: 5lh;
  position: relative;

  .theme-light & {
    background: ${slate[300]};
    border-color: ${slate[500]};
    a,
    div {
      color: ${slate[600]};
    }
  }

  .theme-dark & {
    background: ${forest[600]};
    border-color: ${forest[500]};
    a,
    div {
      color: ${forest[400]};
    }
  }

  div {
    white-space: nowrap;
    padding: 0 1ch;
  }
  div:nth-child(even) {
    .theme-dark & {
      background: ${forest[700]};
    }
    .theme-light & {
      background: ${slate[200]};
    }
  }
`;

export function TxHistory({
  txs,
  cluster,
}: {
  txs: TxSummary[];
  cluster: SolanaCluster;
}) {
  return (
    <TxsContainer className={text["body/sm/regular"]}>
      {txs.map((r) => (
        <div key={r.txHash}>
          [{r.timestamp.time.toLocaleTimeString()}]{" <"}
          <a
            href={solanaTxUrl(r.txHash, cluster)}
            target="_blank"
            key={r.pubkey + r.timestamp.slot + r.data + r.method}
          >
            {r.txHash.slice(0, 20)}
          </a>
          {">"} {r.method}
          {r.data !== "{}" ? `: ${r.data}` : ""}
        </div>
      ))}
    </TxsContainer>
  );
}
