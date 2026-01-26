import { styled } from "@linaria/react";
import React, { PropsWithChildren } from "react";
import { Children } from "react";
import Symbol06 from "../assets/icons/symbol-06.svg?react";
import { c } from "../utils.js";
import { css } from "@linaria/core";
import { iconClass } from "../icon.js";

const Links = styled.div`
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 2px;

  & > a {
    color: var(--color-fg);
  }
`;

const lineHeightSymbol = css`
  height: 1em;
  width: 1em;
`;

export function SymbolSeparatedItems({ children }: PropsWithChildren) {
  return (
    <Links>
      {Children.toArray(children).map((link, i) => (
        <React.Fragment key={i}>
          {i !== 0 && <Symbol06 className={c(lineHeightSymbol, iconClass)} />}
          {link}
        </React.Fragment>
      ))}
    </Links>
  );
}
