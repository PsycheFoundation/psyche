import { styled } from "@linaria/react";
import { CopyToClipboard } from "./CopyToClipboard.js";
import LinkIcon from "../assets/icons/link.svg?react";
import { css } from "@linaria/core";
import { solanaAccountUrl, SolanaCluster } from "../utils.js";

const Container = styled.div`
  display: flex;
  width: 100%;
`;

const Collapsible = styled.div`
  text-overflow: ellipsis;
  flex-shrink: 1;
  overflow: hidden;
  white-space: nowrap;
  text-align-last: justify;
`;

const linkIcon = css`
  display: flex;
  height: 100%;
  align-items: center;
  svg {
    height: 1em;

    path {
      stroke: currentColor;
    }
  }
`;

export function Address({
  address,
  cluster,
  copy = true,
}: {
  cluster: SolanaCluster;
  address: string;
  copy?: boolean;
}) {
  return (
    <Container>
      {address.slice(0, 4)}
      <Collapsible>{address.slice(4, -4)}</Collapsible>
      {address.slice(-4)}
      {copy && <CopyToClipboard text={address} />}
      <a href={solanaAccountUrl(address, cluster)} target="_blank">
        <div className={linkIcon}>
          <LinkIcon />
        </div>
      </a>
    </Container>
  );
}
