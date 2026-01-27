import { randomUUID } from "crypto";
import { promises as fsp } from "fs";
import { tmpdir } from "os";
import { dirname, join } from "path";
import { Pubkey, Result } from "solana-kiss";

export function utilsGetStateDirectory() {
  return process.env["STATE_DIRECTORY"] ?? `${process.cwd()}/state`;
}

export function utilsGetEnv(name: string, description: string) {
  const value = process.env[name];
  if (!value) {
    throw new Error(`Missing ${description} in environment: ${name}`);
  }
  return value;
}

export function utilLogWithTimestamp(
  programAddress: Pubkey,
  message: string,
  durationMs?: number,
) {
  console.log(
    new Date().toISOString(),
    programAddress.toString().padEnd(44, " "),
    ">",
    message.padEnd(30, " "),
    durationMs !== undefined ? `[duration: ${durationMs}ms]` : "",
  );
}

export async function utilRunInParallel<Input, Output>(
  inputs: Iterable<Input>,
  processor: (input: Input) => Promise<Output>,
): Promise<Array<{ input: Input; result: Result<Output> }>> {
  const promises = [];
  for (const input of inputs) {
    promises.push(
      (async () => {
        try {
          return { input, result: { value: await processor(input) } };
        } catch (error) {
          return { input, result: { error } };
        }
      })(),
    );
  }
  return await Promise.all(promises);
}

export function utilsBigIntMax(a: bigint, b: bigint): bigint {
  return a > b ? a : b;
}
export function utilsBigIntMin(a: bigint, b: bigint): bigint {
  return a < b ? a : b;
}

export function utilsBigintArraySortAscending<Content>(
  array: Array<Content>,
  getKey: (item: Content) => bigint,
): void {
  array.sort((a, b) => {
    const aKey = getKey(a);
    const bKey = getKey(b);
    if (aKey < bKey) {
      return -1;
    }
    if (aKey > bKey) {
      return 1;
    }
    return 0;
  });
}

export async function utilsWritePointPlot(
  directory: string,
  subject: string,
  category: string,
  points: { x: number; y: number }[],
  xLabel?: (x: number) => string,
): Promise<void> {
  const size = { x: 66, y: 14 };
  const pointsCleaned = points.filter(
    (p) => Number.isFinite(p.x) && Number.isFinite(p.y),
  );
  const minX = Math.min(...pointsCleaned.map((p) => p.x));
  const maxX = Math.max(...pointsCleaned.map((p) => p.x));
  const minY = Math.min(...pointsCleaned.map((p) => p.y));
  const maxY = Math.max(...pointsCleaned.map((p) => p.y));
  if (minX >= maxX || minY >= maxY) {
    return;
  }
  function gridPos(point: { x: number; y: number }) {
    return {
      x: Math.round(((point.x - minX) / (maxX - minX)) * (size.x - 1)),
      y: Math.round(((point.y - minY) / (maxY - minY)) * (size.y - 1)),
    };
  }
  const grid = Array.from({ length: size.y }, () => Array(size.x).fill(0));
  for (const pointCleaned of pointsCleaned) {
    const pos = gridPos(pointCleaned);
    grid[pos.y]![pos.x]! += 1;
  }
  const peak = Math.max(...grid.flat());
  const title = `${subject} - ${category}`;
  const metaLeft = `@ ${new Date().toISOString()}`;
  const metaRight = `${points.length.toString()} X`;
  const instensities = " .:-=+*#%@";
  const lines: Array<string> = [];
  lines.push(
    `${metaLeft.padEnd(size.x - metaRight.length + 2, " ")}${metaRight}`,
  );
  lines.push(`+${"-".repeat(size.x)}+`);
  lines.push(
    `|${title.padStart(size.x / 2 + title.length / 2, " ").padEnd(size.x)}|`,
  );
  lines.push(`+${"-".repeat(size.x)}+ ---`);
  for (let rowIndex = size.y - 1; rowIndex >= 0; rowIndex--) {
    const pixels = [];
    for (let colIndex = 0; colIndex < grid[rowIndex]!.length; colIndex++) {
      const value = grid[rowIndex]![colIndex]!;
      const pixel = Math.round((value / peak) * (instensities.length - 1));
      pixels.push(instensities[pixel]);
    }
    const data = `|${pixels.join("")}|`;
    const labelY = (rowIndex / (size.y - 1)) * (maxY - minY) + minY;
    lines.push(`${data} ${labelY.toPrecision(5)}`);
  }
  lines.push(`+${"-".repeat(size.x)}+ ---`);
  const hx = size.x / 2 - 1;
  const labelMinX = xLabel ? xLabel(minX) : minX.toString();
  const labelMaxX = xLabel ? xLabel(maxX) : maxX.toString();
  lines.push(`| ${labelMinX.padEnd(hx, " ")}${labelMaxX.padStart(hx, " ")} |`);
  const plotContent = lines.join("\n") + "\n";
  const plotPath = join(
    utilsGetStateDirectory(),
    `plots`,
    directory,
    subject,
    `${title}.txt`,
  );
  await utilsFsWrite(plotPath, plotContent);
}

export async function utilsFsWrite(
  filePath: string,
  content: string,
): Promise<void> {
  const fileTmpPath = join(tmpdir(), `${randomUUID()}.tmp`);
  await fsp.writeFile(fileTmpPath, content, { flush: true });
  await fsp.mkdir(dirname(filePath), { recursive: true });
  await fsp.rename(fileTmpPath, filePath);
}

export async function utilsFsRead(filePath: string): Promise<string> {
  return await fsp.readFile(filePath, { encoding: "utf-8" });
}
