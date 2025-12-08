function normalize(arr: number[]) {
	const magnitude = Math.sqrt(arr.reduce((sum, val) => sum + val * val, 0))
	for (let i = 0; i < arr.length; i++) {
		arr[i] = arr[i] / magnitude
	}
}

export function makeIcosphere(subdivisions: number = 0) {
	const positions: Point[] = []
	const faces: Point[] = []
	const t = 0.5 + Math.sqrt(5) / 2

	positions.push(
		[-1, +t, 0],
		[+1, +t, 0],
		[-1, -t, 0],
		[+1, -t, 0],
		[0, -1, +t],
		[0, +1, +t],
		[0, -1, -t],
		[0, +1, -t],
		[+t, 0, -1],
		[+t, 0, +1],
		[-t, 0, -1],
		[-t, 0, +1]
	)

	faces.push(
		[0, 11, 5],
		[0, 5, 1],
		[0, 1, 7],
		[0, 7, 10],
		[0, 10, 11],
		[1, 5, 9],
		[5, 11, 4],
		[11, 10, 2],
		[10, 7, 6],
		[7, 1, 8],
		[3, 9, 4],
		[3, 4, 2],
		[3, 2, 6],
		[3, 6, 8],
		[3, 8, 9],
		[4, 9, 5],
		[2, 4, 11],
		[6, 2, 10],
		[8, 6, 7],
		[9, 8, 1]
	)

	let complex = {
		cells: faces,
		positions: positions,
	}

	while (subdivisions-- > 0) {
		complex = subdivide(complex)
	}

	complex.positions.forEach(normalize)

	return complex
}

interface Complex {
	cells: Point[]
	positions: Point[]
}

// TODO: work out the second half of loop subdivision
// and extract this into its own module.
function subdivide(complex: Complex): Complex {
	const positions = complex.positions
	const cells = complex.cells

	const newCells: Point[] = []
	const newPositions: Point[] = []
	const midpoints: Record<string, Point> = {}
	let l = 0

	for (let i = 0; i < cells.length; i++) {
		const cell = cells[i]
		const c0 = cell[0]
		const c1 = cell[1]
		const c2 = cell[2]
		const v0 = positions[c0]
		const v1 = positions[c1]
		const v2 = positions[c2]

		const a = getMidpoint(v0, v1)
		const b = getMidpoint(v1, v2)
		const c = getMidpoint(v2, v0)

		let ai = newPositions.indexOf(a)
		if (ai === -1) ((ai = l++), newPositions.push(a))
		let bi = newPositions.indexOf(b)
		if (bi === -1) ((bi = l++), newPositions.push(b))
		let ci = newPositions.indexOf(c)
		if (ci === -1) ((ci = l++), newPositions.push(c))

		let v0i = newPositions.indexOf(v0)
		if (v0i === -1) ((v0i = l++), newPositions.push(v0))
		let v1i = newPositions.indexOf(v1)
		if (v1i === -1) ((v1i = l++), newPositions.push(v1))
		let v2i = newPositions.indexOf(v2)
		if (v2i === -1) ((v2i = l++), newPositions.push(v2))

		newCells.push([v0i, ai, ci], [v1i, bi, ai], [v2i, ci, bi], [ai, bi, ci])
	}

	return {
		cells: newCells,
		positions: newPositions,
	}

	// reuse midpoint vertices between iterations.
	// Otherwise, there'll be duplicate vertices in the final
	// mesh, resulting in sharp edges.
	function getMidpoint(a: Point, b: Point) {
		const point = midpoint(a, b)
		const pointKey = pointToKey(point)
		const cachedPoint = midpoints[pointKey]
		if (cachedPoint) {
			return cachedPoint
		} else {
			return (midpoints[pointKey] = point)
		}
	}

	function pointToKey(point: Point) {
		return (
			point[0].toPrecision(6) +
			',' +
			point[1].toPrecision(6) +
			',' +
			point[2].toPrecision(6)
		)
	}

	function midpoint(a: Point, b: Point): Point {
		return [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2, (a[2] + b[2]) / 2]
	}
}

type Point = [number, number, number]
