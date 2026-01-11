import { ErrorStack, JsonValue } from "solana-kiss";

export async function fetchJson(
  url: string,
  method: string,
  body?: JsonValue,
  headers?: Record<string, string>,
): Promise<JsonValue> {
  const response = await fetch(url, {
    method: method,
    headers: { ...headers, "Content-Type": "application/json" },
    body: body ? JSON.stringify(body) : undefined,
  });
  if (!response.ok) {
    throw new ErrorStack(`${url}: ${response.status} ${await response.text()}`);
  }
  return response.json();
}
