import type {
	Checkpoint,
	HubRepo,
	GcsRepo,
	LearningRateSchedule,
	LLMArchitecture,
} from 'psyche-deserialize-zerocopy-wasm'

// External config schema (matches Rust ExternalModelConfig)
interface ExternalModelConfig {
	version: number
	architecture: LLMArchitecture
	data_type: string
	data_location: unknown
	lr_schedule: LearningRateSchedule
	optimizer: unknown
	run_metadata?: unknown
	client_requirements?: unknown
}

interface ExternalConfigCacheEntry {
	config: ExternalModelConfig
	fetchedAt: number
}

// Cache for external configs to avoid repeated fetches
const configCache = new Map<string, ExternalConfigCacheEntry>()
const CACHE_TTL_MS = 5 * 60 * 1000 // 5 minutes

/**
 * Extract string from FixedString (null-terminated byte array)
 */
function fixedStringToString(fixedStr: { inner: number[] } | string): string {
	if (typeof fixedStr === 'string') {
		return fixedStr
	}
	// Find null terminator and convert to string
	const bytes = fixedStr.inner || fixedStr
	if (Array.isArray(bytes)) {
		const nullIndex = bytes.indexOf(0)
		const relevantBytes = nullIndex >= 0 ? bytes.slice(0, nullIndex) : bytes
		return String.fromCharCode(...relevantBytes)
	}
	return String(bytes)
}

/**
 * Get the external config URL from a checkpoint
 */
function getExternalConfigUrl(checkpoint: Checkpoint): string | null {
	if (typeof checkpoint !== 'object' || checkpoint === null) {
		return null
	}

	// Handle Hub checkpoint
	if ('Hub' in checkpoint && checkpoint.Hub) {
		const hub = checkpoint.Hub as HubRepo
		const repoId = fixedStringToString(hub.repo_id)
		const revision = hub.revision ? fixedStringToString(hub.revision) : 'main'
		// HuggingFace raw content URL
		return `https://huggingface.co/${repoId}/raw/${revision}/config/model_config.json`
	}

	// Handle P2P checkpoint (also uses Hub repo)
	if ('P2P' in checkpoint && checkpoint.P2P) {
		const p2p = checkpoint.P2P as HubRepo
		const repoId = fixedStringToString(p2p.repo_id)
		const revision = p2p.revision ? fixedStringToString(p2p.revision) : 'main'
		return `https://huggingface.co/${repoId}/raw/${revision}/config/model_config.json`
	}

	// Handle Gcs checkpoint
	if ('Gcs' in checkpoint && checkpoint.Gcs) {
		const gcs = checkpoint.Gcs as GcsRepo
		const bucket = fixedStringToString(gcs.bucket)
		const prefix = gcs.prefix ? fixedStringToString(gcs.prefix) : ''
		const pathPrefix = prefix ? `${prefix}/` : ''
		// GCS public URL
		return `https://storage.googleapis.com/${bucket}/${pathPrefix}config/model_config.json`
	}

	// Handle P2PGcs checkpoint
	if ('P2PGcs' in checkpoint && checkpoint.P2PGcs) {
		const gcs = checkpoint.P2PGcs as GcsRepo
		const bucket = fixedStringToString(gcs.bucket)
		const prefix = gcs.prefix ? fixedStringToString(gcs.prefix) : ''
		const pathPrefix = prefix ? `${prefix}/` : ''
		return `https://storage.googleapis.com/${bucket}/${pathPrefix}config/model_config.json`
	}

	// Dummy and Ephemeral checkpoints don't have external config
	return null
}

/**
 * Fetch external config from the checkpoint's config URL
 */
export async function fetchExternalConfig(
	checkpoint: Checkpoint
): Promise<ExternalModelConfig | null> {
	const url = getExternalConfigUrl(checkpoint)
	if (!url) {
		return null
	}

	// Check cache
	const cached = configCache.get(url)
	if (cached && Date.now() - cached.fetchedAt < CACHE_TTL_MS) {
		return cached.config
	}

	try {
		const response = await fetch(url, {
			headers: {
				Accept: 'application/json',
			},
		})

		if (!response.ok) {
			console.warn(
				`Failed to fetch external config from ${url}: ${response.status} ${response.statusText}`
			)
			return null
		}

		const config = (await response.json()) as ExternalModelConfig

		// Validate required fields
		if (!config.architecture || !config.lr_schedule) {
			console.warn(
				`Invalid external config from ${url}: missing architecture or lr_schedule`
			)
			return null
		}

		// Cache the result
		configCache.set(url, {
			config,
			fetchedAt: Date.now(),
		})

		return config
	} catch (error) {
		console.warn(`Error fetching external config from ${url}:`, error)
		return null
	}
}

/**
 * Get architecture from checkpoint (fetches external config if needed)
 */
export async function getArchitectureFromCheckpoint(
	checkpoint: Checkpoint
): Promise<LLMArchitecture | null> {
	const config = await fetchExternalConfig(checkpoint)
	return config?.architecture ?? null
}

/**
 * Get LR schedule from checkpoint (fetches external config if needed)
 */
export async function getLRScheduleFromCheckpoint(
	checkpoint: Checkpoint
): Promise<LearningRateSchedule | null> {
	const config = await fetchExternalConfig(checkpoint)
	return config?.lr_schedule ?? null
}

/**
 * Clear the config cache (useful for testing or manual refresh)
 */
export function clearExternalConfigCache(): void {
	configCache.clear()
}
