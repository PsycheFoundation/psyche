import { load } from 'cheerio'
import { readFileSync } from 'fs'

export function parseHtmlTable(folder: string, tabName: string) {
	const parsedRows = []
	const htmlPath = folder + '/' + tabName + '.html'
	const $ = load(readFileSync(htmlPath, 'utf8'))
	const htmlRows = $('tbody > tr')
	for (let htmlRowIndex = 0; htmlRowIndex < htmlRows.length; htmlRowIndex++) {
		const parsedCells = []
		const htmlRow = $(htmlRows[htmlRowIndex])
		const htmlCells = htmlRow.find('td')
		for (
			let htmlCellIndex = 0;
			htmlCellIndex < htmlCells.length;
			htmlCellIndex++
		) {
			const htmlCell = $(htmlCells[htmlCellIndex])
			const htmlInfo = htmlCell.find('a').attr('href') ?? htmlCell.text()
			if (htmlInfo) {
				parsedCells.push(htmlInfo)
			} else {
				break
			}
		}
		parsedRows.push(parsedCells)
	}
	return parsedRows
}
