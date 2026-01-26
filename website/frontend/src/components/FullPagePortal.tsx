import { PropsWithChildren, useEffect } from "react";
import ReactDOM from "react-dom";
import { styled } from "@linaria/react";

export function FullPagePortal({
  children,
  open = false,
}: PropsWithChildren<{ open?: boolean }>) {
  useEffect(() => {
    if (open) {
      document.body.style.overflow = "hidden";
    } else {
      document.body.style.overflow = "";
    }

    return () => {
      document.body.style.overflow = "";
    };
  }, [open]);

  if (!open) {
    return children;
  }

  return (
    <>
      {ReactDOM.createPortal(
        <PortalOverlay>
          <PortalContent>{children}</PortalContent>
        </PortalOverlay>,
        document.getElementById("outlet")!,
      )}
    </>
  );
}

const PortalOverlay = styled.div`
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  z-index: 9999;

  background-color: var(--color-bg);
`;

const PortalContent = styled.div`
  width: 100%;
  height: 100%;
  & > * {
    overflow: auto;
  }
`;
