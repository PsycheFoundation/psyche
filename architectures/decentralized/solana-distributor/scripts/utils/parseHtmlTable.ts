import { load } from "cheerio";
import { readFileSync } from "fs";

export function parseHtmlTable(folder: string, tabName: string) {
  const parsedRows = [];
  const htmlPath = folder + "/" + tabName + ".html";
  const $ = load(readFileSync(htmlPath, "utf8"));
  const htmlRows = $("tbody > tr");
  for (let htmlRowIndex = 2; htmlRowIndex < htmlRows.length; htmlRowIndex++) {
    const parsedCells = [];
    const htmlRow = $(htmlRows[htmlRowIndex]);
    const htmlCells = htmlRow.find("td");
    for (
      let htmlCellIndex = 0;
      htmlCellIndex < htmlCells.length;
      htmlCellIndex++
    ) {
      const htmlCell = $(htmlCells[htmlCellIndex]);
      parsedCells.push(htmlCell.find("a").attr("href") ?? htmlCell.text());
    }
    parsedRows.push(parsedCells);
  }
  return parsedRows;
}
