export interface Format {
	replacer(this: any, key: string): any
	reviver(key: string, value: any): any
}

export type Version = 1 | 'unversioned'
export const CURRENT_VERSION = 1 satisfies Version
export type CurrentVersion = typeof CURRENT_VERSION
