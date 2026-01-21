import { styled } from "@linaria/react";
import { PropsWithChildren } from "react";
import { text } from "../fonts.js";
import { forest, slate } from "../colors.js";

const Chit = styled.div`
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: space-between;
  padding: 4px 8px;
  box-shadow:
    inset -1px -1px 0px rgba(0, 0, 0, 0.5),
    inset 1px 1px 0px rgba(255, 255, 255, 0.5);
`;

const ChitValue = styled.span`
  padding: 2px 4px;

  .theme-dark & {
    color: ${slate[0]};
  }
`;

const ChitLabel = styled.span`
  color: ${slate[600]};
  .theme-dark & {
    color: ${forest[300]};
  }
`;

export function InfoChit({
  label,
  children,
}: PropsWithChildren<{ label: string }>) {
  return (
    <Chit>
      <ChitValue className={text["body/sm/regular"]}>{children}</ChitValue>
      <ChitLabel className={text["aux/xs/regular"]}>{label}</ChitLabel>
    </Chit>
  );
}
