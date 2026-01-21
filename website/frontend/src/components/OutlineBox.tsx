import { css } from "@linaria/core";
import { styled } from "@linaria/react";
import { type PropsWithChildren, type ReactNode } from "react";
import { text } from "../fonts.js";
import { forest, slate } from "../colors.js";
import { c } from "../utils.js";

const boxHeaderChild = css`
  padding: 0 0.4ch;

  .theme-light & {
    color: ${forest[700]};
  }
  .theme-dark & {
    color: ${forest[300]};
  }
`;

const BoxHeader = styled.legend`
  margin: 0 1ch;
  transform: translateY(-10%);
`;
const BoxContainer = styled.fieldset`
  position: relative;
  border: 2px solid;
  padding: 0;
  flex-shrink: 1;
  flex-grow: 1;
  display: flex;
  flex-direction: column;
  .theme-light & {
    border-color: ${slate[500]};
  }
  .theme-dark & {
    border-color: ${forest[500]};
  }
`;

export function OutlineBox({
  children,
  title,
  className,
  titleClassName,
}: PropsWithChildren<{
  className?: string;
  titleClassName?: string;
  title: ReactNode;
}>) {
  return (
    <BoxContainer className={className}>
      <BoxHeader>
        <span
          className={c(boxHeaderChild, titleClassName ?? text["display/4xl"])}
        >
          {title}
        </span>
      </BoxHeader>
      {children}
    </BoxContainer>
  );
}
