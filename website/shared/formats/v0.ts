import { PublicKey } from '@solana/web3.js'
import { Format } from './type.js'
// NOTE: You CANNOT use information outside of this file for serialization / deserialization
// otherwise you could break old versions

interface PublicKeyJSON {
	___type: 'pubkey'
	value: string
}

interface BigIntJSON {
	___type: 'bigint'
	value: string
}

interface DateJSON {
	___type: 'date'
	value: string
}
interface MapJSON {
	___type: 'map'
	value: Array<[string, any]>
}

interface SetJSON {
	___type: 'set'
	value: any[]
}

function isBigIntJSON(obj: any): obj is BigIntJSON {
	return (
		obj &&
		typeof obj === 'object' &&
		obj.___type === 'bigint' &&
		typeof obj.value === 'string'
	)
}

function isDateJSON(obj: any): obj is DateJSON {
	return (
		obj &&
		typeof obj === 'object' &&
		obj.___type === 'date' &&
		typeof obj.value === 'string'
	)
}

function isMapJson(obj: any): obj is MapJSON {
	return (
		obj &&
		typeof obj === 'object' &&
		obj.___type === 'map' &&
		Array.isArray(obj.value)
	)
}

function isSetJson(obj: any): obj is SetJSON {
	return (
		obj &&
		typeof obj === 'object' &&
		obj.___type === 'set' &&
		Array.isArray(obj.value)
	)
}

function isPublicKeyJson(obj: any): obj is PublicKeyJSON {
	return (
		obj &&
		typeof obj === 'object' &&
		obj.___type === 'pubkey' &&
		typeof obj.value === 'string'
	)
}

function isBN(obj: any) {
	return (
		obj &&
		typeof obj === 'object' &&
		(('negative' in obj && 'words' in obj && 'length' in obj && 'red' in obj) ||
			'_bn' in obj)
	)
}
function isPublicKey(obj: any) {
	return (
		obj &&
		typeof obj === 'object' &&
		'constructor' in obj &&
		'findProgramAddressSync' in obj.constructor
	)
}

function psycheJsonReplacer(this: any, key: string): any {
	const value = this[key]
	if (isPublicKey(value)) {
		return {
			___type: 'pubkey',
			value: value.toString(),
		}
	}
	if (typeof value === 'bigint' || isBN(value)) {
		return {
			___type: 'bigint',
			value: value.toString(),
		}
	}
	if (value instanceof Date) {
		return {
			___type: 'date',
			value: value.toString(),
		}
	}
	if (value instanceof Map) {
		return {
			___type: 'map',
			value: [...value.entries()],
		}
	}
	if (value instanceof Set) {
		return {
			___type: 'set',
			value: [...value.values()],
		}
	}
	return value
}

function psycheJsonReviver(_key: string, value: any): any {
	if (isPublicKeyJson(value)) {
		return new PublicKey(value.value)
	}
	if (isBigIntJSON(value)) {
		return BigInt(value.value)
	}
	if (isDateJSON(value)) {
		return new Date(value.value)
	}
	if (isMapJson(value)) {
		return new Map(value.value)
	}
	if (isSetJson(value)) {
		return new Set(value.value)
	}
	return value
}

export const format: Format = {
	replacer: psycheJsonReplacer,
	reviver: psycheJsonReviver,
}
