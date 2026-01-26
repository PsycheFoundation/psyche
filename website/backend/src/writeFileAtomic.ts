import wfa from "write-file-atomic";

export function writeFileAtomic(
  filename: string,
  data: Buffer | string,
): Promise<void> {
  return new Promise((res, rej) =>
    wfa(filename, data, (err) => {
      if (err) {
        rej(err);
      } else {
        res();
      }
    }),
  );
}
