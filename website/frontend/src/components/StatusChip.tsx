import { styled } from "@linaria/react";
import { forest, gold, lime, slate, success } from "../colors.js";
import { text } from "../fonts.js";
import { c } from "../utils.js";
import { css } from "@linaria/core";
import { RunStatus } from "shared";
import { PropsWithChildren } from "react";

type Style = "bold" | "minimal";

const colors: Record<RunStatus["type"], string> = {
  active: success[400],
  funding: lime[300],
  completed: forest[500],
  paused: slate[400],
  waitingForMembers: gold[300],
};

const labels: Record<RunStatus["type"], string> = {
  active: "active",
  funding: "funding",
  completed: "completed",
  paused: "paused",
  waitingForMembers: "waiting for compute",
};

const StatusChipContainer = styled.div`
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  text-transform: uppercase;
  color: ${(props) => (props.inverted ? "var(--color-bg)" : "var(--color-fg)")};
`;

const Dot = styled.span`
  height: 1em;
  width: 1em;
  background-color: ${({ color }) => color};
  border-radius: 100%;
  display: inline-block;
  position: relative;

  &::after {
    content: "";
    position: absolute;
    top: 0;
    left: 0;
    height: 100%;
    width: 100%;
    background-color: ${({ color }) => color};
    border-radius: 100%;
    opacity: 0.5;
    animation: pulse 2s ease-in-out infinite;
    display: ${({ active }) => (active ? "block" : "none")};
  }

  @keyframes pulse {
    0% {
      transform: scale(1);
      opacity: 0.5;
    }
    100% {
      transform: scale(2);
      opacity: 0;
    }
  }
`;

const Bold = css`
  color: var(--color-bg);
  background: var(--color-fg);
`;
const BoldInverted = css`
  color: var(--color-fg);
  background: var(--color-bg);
`;

export function StatusChip({
  status,
  style,
  inverted,
  children,
}: PropsWithChildren<{
  status: RunStatus["type"];
  style: Style;
  inverted?: boolean;
}>) {
  return (
    <StatusChipContainer
      inverted={inverted}
      className={c(
        text["body/sm/medium"],
        style === "bold" && (inverted ? BoldInverted : Bold),
      )}
    >
      <Dot color={colors[status]} active={status === "active"} />
      {children ?? labels[status]}
    </StatusChipContainer>
  );
}
