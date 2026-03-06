const IGNORE_ALLOWLIST_LOCALSTORAGE_KEY = '__psyche_ignore_allowlist'

declare global {
	interface Window {
		setIgnoreRunAllowlist(ignore: any): void
	}
}
const ignoreAllowlistLocalStorageValue = window.localStorage.getItem(
	IGNORE_ALLOWLIST_LOCALSTORAGE_KEY
)

window.setIgnoreRunAllowlist = (ignore) => {
	if (!(typeof ignore === 'boolean')) {
		console.error(
			`setIgnoreRunAllowlist called with non-boolean argument ${ignore}. pass true or false.`
		)
	}
	window.localStorage.setItem(IGNORE_ALLOWLIST_LOCALSTORAGE_KEY, `${ignore}`)
	window.location.reload()
}

// default to ignoring in development mode
if (
	import.meta.env.MODE === 'development' &&
	ignoreAllowlistLocalStorageValue === null
) {
	window.setIgnoreRunAllowlist(true)
}

const ignoreRunAllowlist = ignoreAllowlistLocalStorageValue === 'true'

// any run ID outside this list will not be displayed in the frontend in the summary list
const ALLOWLISTED_RUN_IDS = ignoreRunAllowlist
	? null
	: [
			'consilience-40b-1',
			'hermes-3-8b',
			'hermes-3-8b-2',
			'hermes-4-8b',
			'hermes-4-8b-2',
			'dm-fwedu-baseline',
			'dm-fwedu-baseline-2',
			'dm-dclm-baseline',
			'dm-fwedu-dclm',
			'dm-fwedu-dclm-fpdf',
			'dm-fwedu-dclm-fw2hq',
			'dm-fwedu-dclm-stack',
			'dm-fwedu-dclm-stack-nmath',
			'dm-fwedu-dclm-wiki-pes',
			'dm-consilience-rc1',
			'dm-consilience-rc2',
			'dm-consilience-rc3',
			'dm-consilience-rc4',
			'hermes-4-36b',
			'hermes-4.1-36b',
			'hermes-4.3-36b',
			'hermes-4.3-36b-2',
			'moe-10b-a1b-8k-wsd-lr3e4-1t',
			'mormio-qwen30b-science-sft',
		]

export function shouldDisplayRun(runId: string) {
	return ALLOWLISTED_RUN_IDS === null || ALLOWLISTED_RUN_IDS.includes(runId)
}
