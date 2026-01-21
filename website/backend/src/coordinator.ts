export type UniqueRunKey = `${string}-${string}` & { __uniqueRunKey: true };

export function runKey(runId: string, index: number): UniqueRunKey {
  return `${runId}-${index}` as UniqueRunKey;
}

export function getRunFromKey(
  runKey: UniqueRunKey,
): [runId: string, index: number] {
  const [runId, index] = splitAtLastInstance(runKey, "-");
  return [runId, Number.parseInt(index, 10)];
}

function splitAtLastInstance(text: string, splitAt: string): [string, string] {
  var index = text.lastIndexOf(splitAt);
  return [text.slice(0, index), text.slice(index + 1)];
}
