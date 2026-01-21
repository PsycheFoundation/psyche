import { createFileRoute } from "@tanstack/react-router";
import { Header } from "../components/Header.js";
import { Footer } from "../components/Footer.js";
import { styled } from "@linaria/react";
import { text } from "../fonts.js";
import { Button } from "../components/Button.js";
import ArrowLeft from "../assets/icons/arrow-left.svg?react";

export const Route = createFileRoute("/legal")({
  component: RouteComponent,
});

const Main = styled.main`
  max-width: 80ch;
  padding: 4ch;
  text-align: justify;
  margin: 0 auto;
  & h1 {
    font-size: 1rem;
  }
`;

const PageContainer = styled.div`
  display: flex;
  flex-direction: column;
  min-height: 100vh;
`;

function RouteComponent() {
  return (
    <PageContainer>
      <Header />
      <Main className={text["aux/sm/regular"]}>
        <Button
          to={"/"}
          style="action"
          icon={{
            side: "left",
            svg: ArrowLeft,
          }}
        >
          Back
        </Button>
        <h1>Psyche Phase 0: Testnet Legal Disclaimer</h1>
        <p>
          The Psyche Testnet blockchain software (the “<b>Testnet</b>
          ”) is a screened, centralized blockchain testing environment provided
          as a free service by Nous Research Inc. (“
          <b>Nous</b>”) on behalf of the Psyche Foundation for the purpose of
          enabling users (software developers, operators, stakers, and others)
          to test the Testnet and any services on the Testnet (“<b>Services</b>
          ”) in a non-production environment for testing and engineering
          purposes.
        </p>
        <p>
          All information and materials published, distributed or otherwise made
          available on the Testnet including the Services, either by Nous, the
          Psyche Foundation, or others, are provided for non-commercial,
          personal use, and testing purposes only.
        </p>
        <p>
          While the Testnet uses cryptographic incentive mechanisms to motivate
          GPU owners to contribute their resources towards model training, any
          digital protocol tokens made available by Nous or the Psyche
          Foundation on the Testnet, any tokens configured using the Testnet,
          and any tokens configured using any extrinsics available for the
          Testnet have no economic or monetary value and cannot be exchanged for
          or converted into cash, cash equivalent, or value.
        </p>
        <p>
          By using the Testnet and Services, you acknowledge and agree that use
          of the Testnet and Services is entirely at your own risk.
        </p>
        <p>
          All content provided on the Testnet, including the Services, is
          provided on an “as-is” and “as available” basis, without any
          representations or warranties of any kind and all implied terms are
          excluded to the fullest extent permitted by law. No party involved in,
          or having contributed to the development and operation of, the Testnet
          and the Services, including but not limited to, Nous, the Psyche
          Foundation, any operators and stakers on the Testnet, and any
          affiliates, directors, employees, contractors, service providers or
          agents of the foregoing (the “<b>Parties Involved</b>”) accept any
          responsibility or liability to you or to any third parties in relation
          to any materials or information accessed or downloaded via the Testnet
          and the Services. You acknowledge and agree that the Parties Involved
          are not responsible for any damage to your computer systems, loss of
          data, or any other loss or damage resulting from your use of the
          Testnet or the Services.
        </p>
        <p>
          The Parties Involved hereby disclaim all warranties of any kind,
          whether express or implied, statutory or otherwise, including but not
          limited to any warranties of merchantability, non-infringement and
          fitness for a particular purpose.
        </p>
        <p>
          To the fullest extent permitted by law, in no event shall the Parties
          Involved have any liability whatsoever to any person for any direct or
          indirect loss, liability, cost, claim, expense or damage of any kind,
          whether in contract or in tort, including negligence, or otherwise,
          arising out of or related to the use of all or part of the Testnet and
          the Services, even if the Parties Involved were advised of the
          possibility of such damages.
        </p>
        <p>
          The Testnet is not an offer to sell or solicitation of an offer to buy
          any security or other regulated financial instrument. The Parties
          Involved are not providing technical, investment, financial,
          accounting, tax, legal or other advice. Please consult your own
          professionals and conduct your own research before connecting to or
          interacting with the Testnet, the Services, or any third party, or
          making any investment or financial decisions.
        </p>
        <p>
          The PsycheFoundation GitHub, the Psyche.Network documents site, and
          all articles on this site are provided for technical support purposes
          only, without representation, warranty or guarantee of any kind.
        </p>
      </Main>
      <Footer />
    </PageContainer>
  );
}
