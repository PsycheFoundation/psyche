import { useState, useRef } from "react";
import CopyIcon from "../assets/icons/copy.svg?react";
import { styled } from "@linaria/react";

export function CopyToClipboard({ text: copyText }: { text: string }) {
  const [copied, setCopied] = useState(false);
  const timeoutRef = useRef<NodeJS.Timeout | null>(null);

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(copyText);

      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }

      setCopied(true);

      timeoutRef.current = setTimeout(() => {
        setCopied(false);
        timeoutRef.current = null;
      }, 300);
    } catch (err) {
      console.error("Failed to copy text: ", err);
    }
  };

  return (
    <CopyButton
      onClick={handleCopy}
      className={copied ? "copied" : ""}
      aria-label="Copy to clipboard"
    >
      <CopyIcon />
    </CopyButton>
  );
}

const CopyButton = styled.button`
  cursor: pointer;
  position: relative;
  background: transparent;
  border: none;
  display: flex;
  align-items: center;
  svg {
    height: 1em;
    path {
      fill: currentColor;
    }
  }
  color: inherit;
  &.copied {
    color: white;
  }
`;
