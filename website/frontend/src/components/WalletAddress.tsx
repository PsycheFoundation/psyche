export function WalletAddress({ children }: { children: string }) {
  return (
    <span>
      {children.slice(0, 4)}...
      {children.slice(-4)}
    </span>
  );
}
