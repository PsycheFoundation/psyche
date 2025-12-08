import { Format, Version, CURRENT_VERSION, CurrentVersion } from './type.js'
import { format as v0 } from './v0.js'
import { format as v1 } from './v1.js'

export const formats: Record<Version, Format> = {
	unversioned: v0,
	'1': v1,
	'2': v1,
}
export type { Format, Version, CurrentVersion }
export { CURRENT_VERSION }
