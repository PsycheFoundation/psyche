import { ErrorComponentProps } from "@tanstack/react-router";
import { styled } from "@linaria/react";
import { text } from "../fonts.js";
import { Footer } from "./Footer.js";

const Outer = styled.div`
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  height: 100vh;
`;
const Container = styled.div`
  text-align: center;
`;

const ErrorContainer = styled.pre`
  max-width: 600px;
  text-align: left;
  margin: 0 auto;
  border: 4px solid var(--color-fg);
  text-wrap: wrap;
  padding: 8px;
  margin-top: 8px;
`;
export function SiteBroken({ error }: ErrorComponentProps) {
  return (
    <Outer>
      <Container>
        <div className={text["display/6xl"]}>something went wrong</div>
        <div className={text["aux/base/bold"]}>
          send this error to someone on the psyche team:
        </div>
        <ErrorContainer className={text["button/sm"]}>
          {error.name}
          {"\n"}
          {error.message}
        </ErrorContainer>
        <div className="grow" />
      </Container>
      <Footer />
    </Outer>
  );
}
