import { Connection, PublicKey, type AccountInfo } from '@solana/web3.js'
import { sha256 } from '@noble/hashes/sha256'

// Program IDs
const COORDINATOR_PROGRAM_ID = new PublicKey(
	'4SHugWqSXwKE5fqDchkJcPEqnoZE22VYKtSTVm7axbT7'
)
const AUTHORIZER_PROGRAM_ID = new PublicKey(
	'PsyAUmhpmiUouWsnJdNGFSX8vZ6rWjXjgDPHsgqPGyw'
)

const SCOPE = Buffer.from('CoordinatorJoinRun')

// Byte offsets (derived from Borsh layout — see GET_AUTHORIZATION_PLAN.md)
const COORDINATOR_JOIN_AUTHORITY_OFFSET = 41
const AUTH_ACTIVE_OFFSET = 95 // for standard scope length (18 bytes)
const AUTH_DELEGATES_LEN_OFFSET = 96
const AUTH_DELEGATES_OFFSET = 100

/**
 * Derive the CoordinatorInstance PDA and read its `join_authority` field.
 */
async function getJoinAuthority(
	connection: Connection,
	runId: string
): Promise<PublicKey> {
	const [coordinatorPda] = PublicKey.findProgramAddressSync(
		[Buffer.from('coordinator'), Buffer.from(runId, 'utf-8')],
		COORDINATOR_PROGRAM_ID
	)

	const accountInfo = await connection.getAccountInfo(coordinatorPda)
	if (!accountInfo) {
		throw new Error(
			`CoordinatorInstance account not found for run_id "${runId}" (PDA: ${coordinatorPda.toBase58()})`
		)
	}

	const joinAuthority = new PublicKey(
		accountInfo.data.subarray(
			COORDINATOR_JOIN_AUTHORITY_OFFSET,
			COORDINATOR_JOIN_AUTHORITY_OFFSET + 32
		)
	)
	return joinAuthority
}

/**
 * Derive an Authorization PDA for a given grantor/grantee pair.
 */
function deriveAuthorizationPda(
	grantor: PublicKey,
	grantee: PublicKey
): PublicKey {
	const [pda] = PublicKey.findProgramAddressSync(
		[Buffer.from('Authorization'), grantor.toBytes(), grantee.toBytes(), SCOPE],
		AUTHORIZER_PROGRAM_ID
	)
	return pda
}

/**
 * Parse an Authorization account and check if the given client is authorized.
 * Returns true if active AND (grantee matches client OR client is in delegates).
 */
function isAuthorizedInAccount(data: Buffer, clientPubkey: PublicKey): boolean {
	const active = data[AUTH_ACTIVE_OFFSET] === 1
	if (!active) return false

	// Check if grantee is the client (direct authorization)
	const grantee = new PublicKey(data.subarray(41, 73))
	if (grantee.equals(clientPubkey)) return true

	// Check if grantee is wildcard (all zeros) — anyone is authorized
	if (grantee.equals(PublicKey.default)) return true

	// Check delegates list for the client
	const delegatesLen = data.readUInt32LE(AUTH_DELEGATES_LEN_OFFSET)
	for (let i = 0; i < delegatesLen; i++) {
		const offset = AUTH_DELEGATES_OFFSET + i * 32
		const delegate = new PublicKey(data.subarray(offset, offset + 32))
		if (delegate.equals(clientPubkey)) return true
	}

	return false
}

export interface AuthorizationResult {
	authorized: boolean
	method?: 'direct' | 'wildcard'
	joinAuthority: PublicKey
}

/**
 * Check if a client pubkey is authorized to join a Psyche training run.
 *
 * Checks two PDAs:
 *   1. Direct authorization: grantee = clientPubkey
 *   2. Wildcard authorization: grantee = Pubkey::default (all zeros)
 */
export async function isClientAuthorized(
	connection: Connection,
	runId: string,
	clientPubkey: PublicKey
): Promise<AuthorizationResult> {
	const joinAuthority = await getJoinAuthority(connection, runId)

	// Derive both PDAs
	const directPda = deriveAuthorizationPda(joinAuthority, clientPubkey)
	const wildcardPda = deriveAuthorizationPda(joinAuthority, PublicKey.default)

	// Fetch both accounts in parallel
	const [directAccount, wildcardAccount] = await Promise.all([
		connection.getAccountInfo(directPda),
		connection.getAccountInfo(wildcardPda),
	])

	// Check direct authorization
	if (
		directAccount &&
		isAuthorizedInAccount(directAccount.data, clientPubkey)
	) {
		return { authorized: true, method: 'direct', joinAuthority }
	}

	// Check wildcard authorization
	if (
		wildcardAccount &&
		isAuthorizedInAccount(wildcardAccount.data, clientPubkey)
	) {
		return { authorized: true, method: 'wildcard', joinAuthority }
	}

	return { authorized: false, joinAuthority }
}

// --- CLI ---

function usage(): never {
	console.log(`Usage: check-authorization --rpc-url <URL> --run-id <ID> --client <PUBKEY>

Check if a Solana client pubkey is authorized for a Psyche training run.

Options:
  --rpc-url <URL>     Solana RPC endpoint (e.g. http://localhost:8899)
  --run-id <ID>       Training run identifier
  --client <PUBKEY>   Client public key (base58)
  --help              Show this help message`)
	process.exit(0)
}

function parseArgs(argv: string[]): {
	rpcUrl: string
	runId: string
	client: string
} {
	const args = argv.slice(2)
	let rpcUrl = ''
	let runId = ''
	let client = ''

	for (let i = 0; i < args.length; i++) {
		switch (args[i]) {
			case '--help':
			case '-h':
				usage()
				break
			case '--rpc-url':
				rpcUrl = args[++i]
				break
			case '--run-id':
				runId = args[++i]
				break
			case '--client':
				client = args[++i]
				break
			default:
				console.error(`Unknown argument: ${args[i]}`)
				process.exit(1)
		}
	}

	if (!rpcUrl || !runId || !client) {
		console.error('Error: --rpc-url, --run-id, and --client are all required.')
		process.exit(1)
	}

	return { rpcUrl, runId, client }
}

async function main() {
	const { rpcUrl, runId, client } = parseArgs(process.argv)

	let clientPubkey: PublicKey
	try {
		clientPubkey = new PublicKey(client)
	} catch {
		console.error(`Invalid public key: ${client}`)
		process.exit(1)
	}

	const connection = new Connection(rpcUrl, 'confirmed')
	try {
		const result = await isClientAuthorized(connection, runId, clientPubkey)

		if (result.authorized) {
			console.log(`AUTHORIZED (${result.method})`)
			console.log(`  Run ID:          ${runId}`)
			console.log(`  Client:          ${clientPubkey.toBase58()}`)
			console.log(`  Join Authority:  ${result.joinAuthority.toBase58()}`)
			process.exit(0)
		} else {
			console.log('NOT AUTHORIZED')
			console.log(`  Run ID:          ${runId}`)
			console.log(`  Client:          ${clientPubkey.toBase58()}`)
			console.log(`  Join Authority:  ${result.joinAuthority.toBase58()}`)
			process.exit(1)
		}
	} catch (err: any) {
		console.error(`Error: ${err.message}`)
		process.exit(2)
	}
}

main()
