import { styled } from "@linaria/react";
import { forest, lime, slate } from "../colors.js";

const Container = styled.div`
  height: ${(props) => props.chunkHeight + 8}px;
  padding: ${(props) => (props.size === "small" ? 1 : 2)}px;
  background-color: var(--color-bg);
  border: ${(props) => (props.size === "small" ? 1 : 2)}px solid;
  border-color: rgba(0, 0, 0, ${(props) => (props.disabled ? 0.2 : 0.5)})
    rgba(255, 255, 255, ${(props) => (props.disabled ? 0.2 : 0.5)})
    rgba(255, 255, 255, ${(props) => (props.disabled ? 0.2 : 0.5)})
    rgba(0, 0, 0, ${(props) => (props.disabled ? 0.2 : 0.5)});

  & > div {
    height: 100%;
    background-size: ${(props) =>
        props.chunkWidth + (props.chunkSpacing ?? 4)}px
      16px;
    .theme-dark & {
      background-image: linear-gradient(
        to right,
        ${(props) =>
            props.disabled
              ? slate[300]
              : props.size === "big"
                ? lime[300]
                : forest[500]}
          ${(props) => props.chunkWidth}px,
        transparent ${(props) => props.chunkSpacing ?? 4}px
      );
    }
    .theme-light & {
      background-image: linear-gradient(
        to right,
        ${(props) => (props.disabled ? slate[300] : forest[500])}
          ${(props) => props.chunkWidth}px,
        transparent ${(props) => props.chunkSpacing ?? 4}px
      );
    }
  }
`;
export function ProgressBar({
  ratio,
  chunkWidth,
  chunkHeight,
  chunkSpacing,
  size = "normal",
  disabled = false,
}: {
  ratio: number;
  chunkWidth: number;
  chunkHeight: number;
  chunkSpacing?: number;
  size?: "normal" | "big" | "small";
  disabled?: boolean;
}) {
  return (
    <Container
      chunkWidth={chunkWidth}
      chunkHeight={chunkHeight}
      size={size}
      chunkSpacing={chunkSpacing}
      disabled={disabled}
    >
      <div style={{ width: `${Math.min(ratio, 1) * 100}%` }}></div>
    </Container>
  );
}
