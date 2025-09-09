export interface Format {
	replacer(this: any, key: string): any
	reviver(key: string, value: any): any
}

export type Version = 1 | 2 | 'unversioned'
export const CURRENT_VERSION = 2 satisfies Version
export type CurrentVersion = typeof CURRENT_VERSION
